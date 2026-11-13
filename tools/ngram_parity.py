"""Parity for SingleBloomFilter, NGramsDecontFilter/Indexer and UnigramLogProbFilter: datatrove vs datatrove_rs.

Usage: ngram_parity.py <parquet> [--real N] [--fuzz N] [--dups N] [--language KEY] [--unigram-csv FILE] [--workers W]

Bloom: `run()` over the docs in order (kept docs, every stat, excluded docs, `total_shingles`, the saved
`bloom_filter.bloom` bytes). Decont: index files built from spans of the corpus, then per-doc results and `run()` as
in `parity.py` (the exclusion writer records metadata); the indexer's `compute_hashes` per (label, query). Unigram:
per-doc results and `run()` at several thresholds, including ones equal to a doc's mean log-prob.
"""

import argparse
import contextlib
import os
import random
import sys
import tempfile

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import numpy as np  # noqa: E402
import pyarrow.parquet as pq  # noqa: E402
from datatrove.data import Document  # noqa: E402
from datatrove.pipeline.decont import n_grams as dtd  # noqa: E402
from datatrove.pipeline.dedup import bloom_filter as dtb  # noqa: E402
from datatrove.pipeline.filters import unigram_log_probs as dtu  # noqa: E402
from datatrove.utils.hashing import HashConfig  # noqa: E402
from datatrove.utils.text import TextNormConfig  # noqa: E402

from datatrove_rs import bloom_filter as rsb, decont as rsd, unigram as rsu  # noqa: E402
from fuzz import SEPARATORS, fuzz_docs, perturb  # noqa: E402
from parity import check_filter  # noqa: E402
from parity_ref import RecordingWriter, digest, run_results, settle_is_alpha_state  # noqa: E402

RAW = TextNormConfig(lowercase=False, norm_whitespace=False, remove_punctuation=False, norm_numbers=False)
# datatrove masks bit indexes with `& m_bytes`, so only m_bytes with many set bits reach many bits.
BLOOM_CASES = [
    ("Bloom[1MB-1,k5]", {"m_bytes": (1 << 20) - 1, "k": 5}),
    ("Bloom[1MB,k5 (2 bits)]", {"m_bytes": 1 << 20, "k": 5}),
    ("Bloom[1000B,k3,5-gram,0.5]", {"m_bytes": 1000, "k": 3, "n_grams": 5, "duplicate_threshold": 0.5}),
    ("Bloom[37B,k2,2-gram,0]", {"m_bytes": 37, "k": 2, "n_grams": 2, "duplicate_threshold": 0}),
    ("Bloom[64KB-1,expected,3-gram]", {"m_bytes": (1 << 16) - 1, "expected_elements": 1 << 20, "n_grams": 3,
                                       "seed": 7}),
    ("Bloom[sha1,1-gram,raw]", {"m_bytes": 4095, "k": 1, "n_grams": 1, "duplicate_threshold": 0.9,
                                "hash_config": HashConfig(precision=32, hash_fc="sha1"), "norm_config": RAW}),
    ("Bloom[k0]", {"m_bytes": 64, "k": 0, "n_grams": 4}),
    ("Bloom[threshold 1,dates]", {"m_bytes": 4097, "k": 4, "n_grams": 1, "duplicate_threshold": 1,
                                  "norm_config": TextNormConfig(norm_weekdays=True, norm_monthnames=True)}),
]
DECONT_CASES = [
    ("Decont[default]", {}),
    ("Decont[5-gram,query]", {"n_grams": 5, "find_query_ngrams": True}),
    ("Decont[3-gram,sha1-32,no overlap]", {"n_grams": 3, "find_overlap_ngrams": False,
                                           "hash_config": HashConfig(precision=32, hash_fc="sha1")}),
    ("Decont[1-gram,raw,xxhash32]", {"n_grams": 1, "norm_config": RAW,
                                     "hash_config": HashConfig(precision=32, hash_fc="xxhash")}),
]
TASKS = ("suite|arc:easy", "suite|hellaswag", "other task", "x:y:z")


def dup_docs(texts, n, seed=0):
    """Docs built to collide in a Bloom filter: exact and partial copies, concatenations, edits and repeats."""
    rng = random.Random(seed)
    pool = [t for t in texts if t.strip()] or ["a b c d e f g"]
    out = []
    for _ in range(n):
        src = rng.choice(pool + out[-50:])
        words = src.split(" ")
        a = rng.randrange(len(words))
        r = rng.random()
        if r < 0.25:
            doc = src
        elif r < 0.45:
            doc = " ".join(words[a : a + rng.randint(1, 200)])
        elif r < 0.6:
            doc = src + rng.choice(SEPARATORS) + rng.choice(pool)
        elif r < 0.75:
            for _ in range(rng.randint(1, 5)):
                words[rng.randrange(len(words))] = rng.choice(["x", "the", "", "Ünïcode", "42", "—"])
            doc = " ".join(words)
        elif r < 0.9:
            doc = (" ".join(words[a : a + rng.randint(1, 20)]) + rng.choice(SEPARATORS)) * rng.randint(2, 30)
        else:
            doc = rng.choice(pool)
        out.append(doc)
    return out


def bloom_results(cls, cfg, texts, language):
    with tempfile.TemporaryDirectory() as tmp:
        step = cls(tmp, config=dtb.BloomFilterConfig(**cfg), save_bloom_filter=True,
                   exclusion_writer=RecordingWriter(), language=language)
        try:
            ran, stats, excluded = run_results(step, texts)
        except Exception as e:  # noqa: BLE001 — the exception type is part of the behavior under test
            return {"raise": type(e).__name__, "bits": digest(bytes(step.bit_vector))}
        with open(os.path.join(tmp, "bloom_filter.bloom"), "rb") as f:
            saved = digest(f.read())
    return {"run": ran, "stats": stats, "excluded": excluded, "saved": saved, "shingles": step.total_shingles}


def check_bloom(name, cfg, texts, report, language="eng"):
    ref = bloom_results(dtb.SingleBloomFilter, cfg, texts, language)
    got = bloom_results(rsb.SingleBloomFilter, cfg, texts, language)
    kept = [{i for i, _ in r.get("run", [])} for r in (ref, got)]
    bad = sorted(kept[0] ^ kept[1])
    if ref != got:
        report.write(f"## {name}\nkeys differing: {[k for k in ref if ref.get(k) != got.get(k)]} docs {bad[:20]}\n")
    return len(bad), ref == got


@contextlib.contextmanager
def _no_lighteval():
    """The indexer's `__new__` demands lighteval, which only its `run()` uses."""
    import datatrove.pipeline.base as base

    orig, base.check_required_dependencies = base.check_required_dependencies, lambda *a, **k: None
    try:
        yield
    finally:
        base.check_required_dependencies = orig


def make_indexer(cls, config, language):
    with _no_lighteval():
        return cls(tempfile.gettempdir(), config=config, language=language)


def decont_pairs(texts, seed=0):
    """(label, query) pairs cut from `texts` at spaces, so the corpus contains the indexed n-grams."""
    rng = random.Random(seed)
    pairs = []
    for t in texts[::5]:
        words = t.split(" ")
        a = rng.randrange(len(words))
        b = a + rng.randint(1, 60)
        q = max(0, a - rng.randint(0, 30))
        query = " ".join(words[q:a]) if rng.random() < 0.8 else None
        pairs.append((" ".join(words[a:b]), query))
    return pairs


def write_index(folder, pairs, config, language, seed=0):
    """Index files as `NGramsDecontIndexer.run` writes them, hashed by datatrove's indexer."""
    ix = make_indexer(dtd.NGramsDecontIndexer, config, language)
    rng = random.Random(seed)
    hashes = {task: set() for task in TASKS}
    for label, query in pairs:
        hashes[rng.choice(TASKS)].update(ix.compute_hashes(label, query))
    for task, hs in hashes.items():
        path = os.path.join(folder, f"{task.replace(' ', '_')}.index.hashes")
        np.array(list(hs), dtype=config.hash_config.np_descr).tofile(path)


def check_decont(name, cfg, texts, report, language="eng", workers=1):
    config = dtd.NGramsDecontConfig(**cfg)
    pairs = decont_pairs(texts)
    ixs = [make_indexer(c, config, language) for c in (dtd.NGramsDecontIndexer, rsd.NGramsDecontIndexer)]
    bad_ix = [p for p in pairs + [(t, None) for t in texts[:200]] if ixs[0].compute_hashes(*p) != ixs[1].compute_hashes(*p)]
    for p in bad_ix[:5]:
        report.write(f"## {name} indexer\npair={p!r}\n")
    with tempfile.TemporaryDirectory() as tmp:
        write_index(tmp, pairs, config, language)
        kwargs = {"index_folder": tmp, "config": config, "language": language}
        bad, run_ok = check_filter(name, dtd.NGramsDecontFilter, rsd.NGramsDecontFilter, kwargs, texts, report, workers)
    return bad + len(bad_ix), run_ok


def synthetic_unigram_csv(path, texts, seed=0):
    """A `unigram_freq.csv` holding most corpus words (some cased, so lookups must lowercase) with Zipf-ish counts."""
    import csv

    rng = random.Random(seed)
    words = sorted({w for t in texts for w in t.split()})
    with open(path, "w", encoding="utf-8", newline="") as f:
        out = csv.writer(f)
        out.writerow(["word", "count"])
        for w in words:
            if rng.random() < 0.8:
                out.writerow([w.lower() if rng.random() < 0.9 else w, int(1e9 / (1 + rng.paretovariate(0.7)))])


@contextlib.contextmanager
def unigram_table(path=None, texts=()):
    """datatrove's UnigramLogProbFilter reads `path` (default: `synthetic_unigram_csv(texts)`) instead of downloading
    its table. Workers forked inside the block inherit this."""
    orig = dtu.cached_assets_path
    with tempfile.TemporaryDirectory() as folder:
        csv_path = os.path.join(folder, "unigram_freq.csv")
        if path:
            os.symlink(os.path.abspath(path), csv_path)
        else:
            synthetic_unigram_csv(csv_path, texts)
        dtu.cached_assets_path = lambda **_: folder
        try:
            yield
        finally:
            dtu.cached_assets_path = orig


def check_unigram(texts, report, language="eng", workers=1, batch_sizes=(1,)):
    """At -10 (an int, datatrove's default), other floats, a doc's exact mean (dropped: the test is `>`) and the
    largest mean's predecessor."""
    ref = dtu.UnigramLogProbFilter(language=language)
    means = sorted(float(ref.get_logprob(Document(text=t, id=""))) for t in texts[:50])
    thresholds = [-10, -12.5, -8.0, means[len(means) // 2], float(np.nextafter(means[-1], -np.inf)), 0]
    out = []
    for t in thresholds:
        for b in batch_sizes:
            kwargs = {"logprobs_threshold": t, "language": language, "batch_size": b}
            name = f"Unigram[{t!r},batch {b}]"
            bad, ok = check_filter(name, dtu.UnigramLogProbFilter, rsu.UnigramLogProbFilter, kwargs, texts, report,
                                   workers)
            out.append((name, bad, ok))
    return out


def check_all(texts, report, language="eng", workers=1, dups=None, bloom_cases=BLOOM_CASES, decont_cases=DECONT_CASES):
    """(name, mismatched docs, run()+stats identical) per case; Bloom also gets `dups` (default len(texts))
    `dup_docs`. Unigram uses the table installed by `unigram_table`."""
    bloom_texts = texts + dup_docs(texts, len(texts) if dups is None else dups)
    rows = [(n, *check_bloom(n, cfg, bloom_texts, report, language)) for n, cfg in bloom_cases]
    rows += [(n, *check_decont(n, cfg, texts, report, language, workers)) for n, cfg in decont_cases]
    return rows + check_unigram(texts, report, language, workers)


def check_language(texts, report, language):
    """The subset the per-language tests run: two Bloom configs, one decont config, every unigram threshold."""
    with unigram_table(texts=texts):
        return check_all(texts, report, language, bloom_cases=[BLOOM_CASES[0], BLOOM_CASES[3]],
                         decont_cases=[DECONT_CASES[1]])


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("parquet")
    ap.add_argument("--real", type=int, default=2000)
    ap.add_argument("--fuzz", type=int, default=1000)
    ap.add_argument("--dups", type=int, default=2000, help="heavy-duplication docs for the Bloom filter")
    ap.add_argument("--language", default="eng")
    ap.add_argument("--unigram-csv", default=None, help="unigram_freq.csv (default: synthetic, from the corpus)")
    ap.add_argument("--workers", type=int, default=1)
    ap.add_argument("--out", default="ngram_parity_mismatches.txt")
    args = ap.parse_args()

    settle_is_alpha_state(args.language)
    real = pq.read_table(args.parquet, columns=["text"]).slice(0, args.real).column("text").to_pylist()
    texts = real + perturb(real[: args.real // 4]) + fuzz_docs(args.fuzz)
    print(f"{len(texts)} docs ({len(real)} real), +{args.dups} duplication docs for Bloom; language {args.language}")
    if rsd.NGramsDecontFilter(".", language=args.language).rust() is None:
        print(f"note: no Rust path for {args.language!r}; this compares datatrove with itself")
    with unigram_table(args.unigram_csv, real), open(args.out, "w", encoding="utf-8") as report:
        rows = check_all(texts, report, args.language, args.workers, args.dups)
    for name, bad, ok in rows:
        print(f"{name:36} mismatched docs {bad:5}   run()+stats {'identical' if ok else 'DIFFER'}")
    failed = any(bad or not ok for _, bad, ok in rows)
    print(("FAIL, see " + args.out) if failed else "PASS")
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
