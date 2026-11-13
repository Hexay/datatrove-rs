"""Parity of the stats steps: datatrove's vs datatrove_rs's `run()` on the same docs, split into tasks and merged.

Usage: stats_parity.py <parquet> [--real N] [--fuzz N] [--tasks K] [--language KEY] [--only Step,...]
       [--script-fuzz N] [--cjk-fuzz N]

Compares, per config: the docs each task yields (text and metadata items, in order), each task's step stats, every
JSON file written (all groups) and the files `StatsMerger` writes from them, byte for byte, or the raised exception.
Configs vary thresholds, groups (subsets, order), histogram digits and top-k; one feeds token counts Rust can't keep
exactly, so the run continues in datatrove's loop mid-way.
"""

import argparse
import os
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
os.environ.setdefault("TQDM_DISABLE", "1")
import pyarrow.parquet as pq  # noqa: E402
from loguru import logger  # noqa: E402
from datatrove.pipeline import stats as dt  # noqa: E402
from datatrove.pipeline.stats import TopKConfig  # noqa: E402

from datatrove_rs import _core  # noqa: E402
from datatrove_rs import stats as rs  # noqa: E402
from datatrove_rs._base import rust_language  # noqa: E402
from fuzz import fuzz_docs, perturb  # noqa: E402
from parity_ref import settle_is_alpha_state  # noqa: E402
from stats_ref import case_fuzz, diff, make_metadata, run_tasks  # noqa: E402

TOP3 = TopKConfig(top_k_groups=["fqdn", "suffix"], top_k=3)
CASES = [
    ("WordStats", {}),
    ("WordStats", {"short_word_max_chars_threshold": [1, 3, 3], "long_word_max_chars_threshold": [7, 12],
                   "stop_words": ["the", "and", "The", "de"], "histogram_round_digits": 2, "top_k_config": TOP3}),
    ("SentenceStats", {}),
    ("SentenceStats", {"short_sentence_max_chars_threshold": [10, 20], "long_sentence_max_chars_threshold": [50, 200],
                       "histogram_round_digits": 1, "groups_to_compute": ["histogram", "summary"]}),
    ("DocStats", {}),
    ("DocStats", {"histogram_round_digits": 0, "groups_to_compute": ["suffix", "fqdn", "histogram", "suffix"]}),
    ("DocStats", {"histogram_round_digits": 6, "top_k_config": TopKConfig(top_k_groups=["fqdn"], top_k=1)}),
    ("LineStats", {}),
    ("LineStats", {"ignore_empty_lines": True, "max_k_chars_per_line_tresholds": [],
                   "min_k_chars_per_line_thresholds": [5, 80, 5]}),
    ("ParagraphStats", {}),
    ("ParagraphStats", {"ignore_empty_paragraphs": True, "short_paragraph_max_chars_threshold": [50],
                        "long_paragraph_max_chars_threshold": [500, 2000], "histogram_round_digits": 4}),
]
LANGUAGE_STEPS = ("WordStats", "SentenceStats")


def spill_metadata(metadata):
    """Token counts past 2**53 halfway through: Rust hands its counters to datatrove's loop there."""
    half = len(metadata) // 2
    return metadata[:half] + [m | {"token_count": 2**60 + i} for i, m in enumerate(metadata[half:])]


def encodable(text):
    try:
        text.encode()
        return True
    except UnicodeEncodeError:
        return False


def check(name, kwargs, texts, metadata, tasks, report):
    if name in LANGUAGE_STEPS:  # datatrove's tokenizers raise on lone surrogates, which would end the run
        texts, metadata = zip(*[(t, m) for t, m in zip(texts, metadata) if encodable(t)])
    ref = run_tasks(getattr(dt, name), kwargs, texts, metadata, tasks)
    got = run_tasks(getattr(rs, name), kwargs, texts, metadata, tasks)
    problems = diff(ref, got)
    for p in problems[:20]:
        report.write(f"## {name} {kwargs}\n{p}\n")
    return not problems, ref["error"]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("parquet")
    ap.add_argument("--real", type=int, default=2000)
    ap.add_argument("--fuzz", type=int, default=1000)
    ap.add_argument("--tasks", type=int, default=3)
    ap.add_argument("--language", default=None)
    ap.add_argument("--only", default=None, help="comma-separated step names")
    ap.add_argument("--script-fuzz", type=int, default=0, help="wordtok_fuzz docs for a ported tokenizer")
    ap.add_argument("--cjk-fuzz", type=int, default=0, help="cjk_fuzz docs (zh/ja)")
    ap.add_argument("--out", default="stats_mismatches.txt")
    args = ap.parse_args()

    logger.remove()  # StatsMerger logs every folder
    settle_is_alpha_state(args.language or "eng")
    real = pq.read_table(args.parquet, columns=["text"]).slice(0, args.real).column("text").to_pylist()
    texts = real + perturb(real[: args.real // 4]) + fuzz_docs(args.fuzz) + case_fuzz(args.fuzz // 2)
    name = rust_language(args.language or "eng")
    if args.script_fuzz and name in _core.word_tokenizers():
        from wordtok_fuzz import fuzz_docs as script_fuzz

        texts += script_fuzz(real, args.script_fuzz, 0, name)
    if args.cjk_fuzz and name in ("zh", "ja"):
        from cjk_fuzz import fuzz_docs as cjk_docs

        texts += cjk_docs(name, args.cjk_fuzz)
    metadata = make_metadata(len(texts))
    only = set(args.only.split(",")) if args.only else None
    print(f"{len(texts)} docs ({len(real)} real), {args.tasks} tasks, language {args.language or 'default'} "
          f"(Rust tokenizer {name})")
    cases = [(n, kw | ({"language": args.language} if args.language and n in LANGUAGE_STEPS else {}), metadata)
             for n, kw in CASES if not only or n in only]
    if not only or "DocStats" in only:
        cases += [("DocStats", {}, spill_metadata(metadata)), ("DocStats", {}, make_metadata(len(texts), url=False))]
    failed = False
    with open(args.out, "w", encoding="utf-8") as report:
        for step, kwargs, meta in cases:
            t = time.perf_counter()
            ok, error = check(step, kwargs, texts, meta, args.tasks, report)
            failed |= not ok
            shown = {k: v for k, v in kwargs.items() if k != "language"}
            outcome = ("identical" if ok else "DIFFER") + (f" (both raise {error})" if error and ok else "")
            print(f"{step:15} {str(shown)[:70]:72} {outcome:30} {time.perf_counter() - t:5.0f}s")
    print(("FAIL, see " + args.out) if failed else "PASS")
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
