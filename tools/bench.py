"""Throughput of datatrove's Python steps vs datatrove_rs, single thread, same docs.

Usage: bench.py <fineweb.parquet> [n_docs=1500] [reps=3]

Filters are timed via `filter()` on fresh Documents; the chain runs GopherRepetition -> GopherQuality ->
C4Quality -> FineWebQuality through `run()` as a FineWeb-style pipeline would. MinHash times
get_shingles + get_signature. Reports the median of `reps`.
"""

import os
import statistics
import sys
import time

import pyarrow.parquet as pq
from datatrove.data import Document
from datatrove.pipeline import filters as dt
from datatrove.pipeline.dedup import minhash as dtm

import datatrove_rs as rs
from datatrove_rs.minhash import MinhashDedupSignature as RsMinhash

STEPS = [
    ("GopherRepetition", dt.GopherRepetitionFilter, rs.GopherRepetitionFilter),
    ("GopherQuality", dt.GopherQualityFilter, rs.GopherQualityFilter),
    ("C4Quality", dt.C4QualityFilter, rs.C4QualityFilter),
    ("FineWebQuality", dt.FineWebQualityFilter, rs.FineWebQualityFilter),
]


def timed(fn, reps):
    times = []
    for _ in range(reps):
        t = time.perf_counter()
        fn()
        times.append(time.perf_counter() - t)
    return statistics.median(times)


def filter_all(cls, texts):
    step = cls()
    return lambda: [step.filter(Document(text=t, id="0")) for t in texts]


def chain(classes, texts):
    def run():
        docs = (Document(text=t, id=str(i)) for i, t in enumerate(texts))
        for cls in classes:
            docs = cls().run(docs, rank=0)
        return sum(1 for _ in docs)

    return run


def minhash(cls, texts, tmp):
    step = cls(tmp)

    def run():
        for t in texts:
            s = step.get_shingles(t)
            if s.size:
                step.get_signature(s)

    return run


def main():
    path = sys.argv[1]
    n = int(sys.argv[2]) if len(sys.argv) > 2 else 1500
    reps = int(sys.argv[3]) if len(sys.argv) > 3 else 3
    texts = pq.read_table(path, columns=["text"]).slice(0, n).column("text").to_pylist()
    mb = sum(len(t.encode()) for t in texts) / 1e6
    tmp = os.path.join(os.environ.get("TEMP", "/tmp"), "datatrove_rs_bench")
    rows = [(name, filter_all(p, texts), filter_all(r, texts)) for name, p, r in STEPS]
    rows.append(("Chain (run, 4 filters)", chain([s[1] for s in STEPS], texts), chain([s[2] for s in STEPS], texts)))
    rows.append(("MinHash stage 1", minhash(dtm.MinhashDedupSignature, texts, tmp), minhash(RsMinhash, texts, tmp)))

    print(f"{n} docs, {mb:.1f} MB, median of {reps}\n")
    print(f"| Step | datatrove docs/s | datatrove_rs docs/s | speedup |\n|---|---|---|---|")
    for name, py_fn, rs_fn in rows:
        py_fn(), rs_fn()  # warm-up: tokenizer construction, spaCy load
        tp, tr = timed(py_fn, reps), timed(rs_fn, reps)
        print(f"| {name} | {n / tp:,.0f} | {n / tr:,.0f} | {tp / tr:.1f}x |")


if __name__ == "__main__":
    main()
