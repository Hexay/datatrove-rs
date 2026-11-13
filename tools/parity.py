"""Parity: datatrove's Python steps vs datatrove_rs on the same documents.

Usage: parity.py <fineweb.parquet> [--real N] [--fuzz N] [--random-configs K] [--workers W] [--cache DIR] [--out FILE]

Per doc it compares the filter result (or raised exception type) and the resulting doc.text; then both steps'
`run()` over the docs neither raises on, comparing output ids and texts, every stat field and what the exclusion writer
receives. MinHash compares shingles and
signatures per doc and the signature files `run()` writes. Sentence dedup runs stages 1-2-3 over the corpus plus
`--dup-docs` sentence-sharing docs (`sentdedup_ref.py`). The datatrove side runs on W processes and is cached.
"""

import argparse
import os
import sys
import time
from importlib.metadata import version

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import pyarrow.parquet as pq  # noqa: E402
from datatrove.pipeline.dedup import minhash as dtm  # noqa: E402
from datatrove.pipeline.dedup import sentence_dedup as dts  # noqa: E402

from datatrove_rs import _core  # noqa: E402
from datatrove_rs._base import rust_language  # noqa: E402
from datatrove_rs.minhash import MinhashDedupSignature as RsMinhash  # noqa: E402
from datatrove_rs.sentence_dedup import SentenceDedupFilter as RsSentFilter  # noqa: E402
from datatrove_rs.sentence_dedup import SentenceDedupSignature as RsSentSig  # noqa: E402
from fuzz import fuzz_docs, perturb  # noqa: E402
from sentdedup_fuzz import dup_docs  # noqa: E402
from sentdedup_ref import sentdedup_results, signature_diffs, span_safe  # noqa: E402
from wordtok_fuzz import fuzz_docs as script_fuzz  # noqa: E402
from parity_cases import FILTER_CASES, MINHASH_CASES, SENTDEDUP_CASES, random_filter_cases  # noqa: E402
from parity_ref import (  # noqa: E402
    cached,
    digest,
    filter_outcome,
    filter_results,
    make_step,
    minhash_results,
    settle_is_alpha_state,
)

settle_is_alpha_state()


def _corpus_key(texts):
    return (digest("\0".join(texts)), version("datatrove"), version("spacy"), len(texts))


def _report_filter(name, py_cls, rs_cls, kwargs, texts, idxs, report):
    py_step, rs_step = make_step(py_cls, kwargs), make_step(rs_cls, kwargs)
    for i in idxs[:20]:
        report.write(f"## {name} doc {i}\nkwargs={kwargs}\npy={filter_outcome(py_step, texts[i])[0]!r}\n")
        report.write(f"rs={filter_outcome(rs_step, texts[i])[0]!r}\ntext={texts[i][:300]!r}\n")


def check_filter(name, py_cls, rs_cls, kwargs, texts, report, workers=1, cache_dir=None, run_limit=None):
    ref = cached(cache_dir, ("filter-v2", name, repr(kwargs), _corpus_key(texts), run_limit),
                 lambda: filter_results(py_cls, kwargs, texts, workers, run_limit))
    got = filter_results(rs_cls, kwargs, texts, run_limit=run_limit)
    bad = [i for i, (a, b) in enumerate(zip(ref["outcomes"], got["outcomes"])) if a != b]
    _report_filter(name, py_cls, rs_cls, kwargs, texts, bad, report)
    run_ok = all(ref[k] == got[k] for k in ("run", "stats", "excluded"))
    if not run_ok:
        report.write(f"## {name} run/stats\nkwargs={kwargs}\npy={ref['stats']}\nrs={got['stats']}\n")
    return len(bad), run_ok


def check_minhash(name, cfg, texts, report, workers=1, cache_dir=None, language="eng"):
    config = dtm.MinhashConfig(**cfg)
    ref = cached(cache_dir, ("minhash", name, repr(config), language, _corpus_key(texts)),
                 lambda: minhash_results(dtm.MinhashDedupSignature, config, texts, workers, language))
    got = minhash_results(RsMinhash, config, texts, language=language)
    bad = [i for i, (a, b) in enumerate(zip(ref["outcomes"], got["outcomes"])) if a != b]
    for i in bad[:20]:
        report.write(f"## {name} doc {i}\ntext={texts[i][:300]!r}\n")
    return len(bad), ref["files"] == got["files"]


def check_sentdedup(name, case, texts, report, workers=1, cache_dir=None, language="eng"):
    """Returns (docs whose `remove_dup_sentences` differs, names of the other differing results)."""
    key = ("sentdedup-v2", name, repr(case["config"]), case["finder_workers"], language, _corpus_key(texts))
    ref = cached(cache_dir, key, lambda: sentdedup_results(
        dts.SentenceDedupSignature, dts.SentenceDedupFilter, case, texts, language, workers))
    got = sentdedup_results(RsSentSig, RsSentFilter, case, texts, language)
    flat = [[o for part, _ in r["remove"] for o in part] for r in (ref, got)]
    bad = [i for i, (a, b) in enumerate(zip(*flat)) if a != b]
    differ = [k for k in ref if k != "remove" and ref[k] != got[k]]
    if [s for _, s in ref["remove"]] != [s for _, s in got["remove"]]:
        differ.append("remove stats")
    for i in bad[:20]:
        report.write(f"## {name} remove_dup_sentences doc {i}\npy={flat[0][i]!r}\nrs={flat[1][i]!r}\n"
                     f"text={texts[i][:300]!r}\n")
    if differ:
        sig_docs = signature_diffs(dts.SentenceDedupSignature, RsSentSig, case, texts, language)
        report.write(f"## {name} differs in {differ}; get_hashes differs on docs {sig_docs}\n")
    return len(bad), differ


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("parquet")
    ap.add_argument("--real", type=int, default=2000)
    ap.add_argument("--fuzz", type=int, default=3000)
    ap.add_argument("--random-configs", type=int, default=0, help="random threshold configs per filter")
    ap.add_argument("--workers", type=int, default=1)
    ap.add_argument("--run-limit", type=int, default=None, help="docs fed to the sequential run()/stats pass")
    ap.add_argument("--cache", default=os.environ.get("DATATROVE_RS_PARITY_CACHE"),
                    help="directory caching datatrove-side results (default: $DATATROVE_RS_PARITY_CACHE)")
    ap.add_argument("--out", default="parity_mismatches.txt")
    ap.add_argument("--language", default=None, help="datatrove language passed to every step (default: theirs)")
    ap.add_argument("--script-fuzz", type=int, default=0, help="extra wordtok_fuzz docs for a ported tokenizer")
    ap.add_argument("--dup-docs", type=int, default=2000, help="extra sentence-sharing docs for sentence dedup")
    ap.add_argument("--only", default=None, help="comma-separated case name prefixes to run (default: all)")
    args = ap.parse_args()

    def selected(name):
        return not args.only or name.startswith(tuple(args.only.split(",")))

    lang = {"language": args.language} if args.language else {}
    settle_is_alpha_state(args.language or "eng")
    real = pq.read_table(args.parquet, columns=["text"]).slice(0, args.real).column("text").to_pylist()
    texts = real + perturb(real[: args.real // 4]) + fuzz_docs(args.fuzz)
    if args.script_fuzz and (name := rust_language(args.language or "eng")) in _core.word_tokenizers():
        texts += script_fuzz(real, args.script_fuzz, 0, name)
    print(f"{len(texts)} docs: {len(real)} real, {args.real // 4} perturbed, {args.fuzz} fuzz; {args.workers} workers")
    if args.language and not RsMinhash(".", language=args.language).rust():
        print(f"note: no Rust path for language {args.language!r}; this compares datatrove with itself")
    failed = False
    with open(args.out, "w", encoding="utf-8") as report:
        for name, py_cls, rs_cls, kwargs in FILTER_CASES + random_filter_cases(args.random_configs):
            if not selected(name):
                continue
            t = time.perf_counter()
            bad, run_ok = check_filter(
                name, py_cls, rs_cls, kwargs | lang, texts, report, args.workers, args.cache, args.run_limit
            )
            failed |= bool(bad) or not run_ok
            verdict = "identical" if run_ok else "DIFFER"
            print(f"{name:32} mismatched docs {bad:5}   run()+stats {verdict:9} {time.perf_counter() - t:6.0f}s")
        for name, cfg in MINHASH_CASES:
            if not selected(name):
                continue
            t = time.perf_counter()
            bad, files_ok = check_minhash(name, cfg, texts, report, args.workers, args.cache, args.language or "eng")
            failed |= bool(bad) or not files_ok
            verdict = "identical" if files_ok else "DIFFER"
            print(f"{name:32} mismatched docs {bad:5}   sig files   {verdict:9} {time.perf_counter() - t:6.0f}s")
        sd_lang = args.language or "eng"
        if not RsSentSig(".", config=dts.SentDedupConfig(), language=sd_lang).rust():
            print(f"note: no Rust sentence dedup for {sd_lang!r} (default config); datatrove is compared with itself")
        sd_texts = span_safe(texts + dup_docs(real, args.dup_docs), args.language or "eng")
        for name, case in SENTDEDUP_CASES:
            if not selected(name):
                continue
            t = time.perf_counter()
            bad, differ = check_sentdedup(name, case, sd_texts, report, args.workers, args.cache, args.language or "eng")
            failed |= bool(bad or differ)
            verdict = "identical" if not differ else "DIFFER " + ",".join(differ)
            print(f"{name:32} mismatched docs {bad:5}   all stages  {verdict:9} {time.perf_counter() - t:6.0f}s")
    print(("FAIL, see " + args.out) if failed else "PASS")
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
