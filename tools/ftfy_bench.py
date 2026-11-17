"""FTFYFormatter throughput, single thread: datatrove (Python ftfy), the unchanged-text check alone (datatrove_rs
<= 0.1.6: Rust proves "unchanged", ftfy does the rest) and the Rust `fix_text` port. `run()` over the same docs,
median of `reps`.

Usage: ftfy_bench.py [--n N] [--reps R] [--mojibake] corpus.parquet...
"""

import argparse
import random
import statistics
import sys
import time
from pathlib import Path

from datatrove.data import Document
from datatrove.pipeline.formatters import FTFYFormatter as Py

sys.path.insert(0, str(Path(__file__).resolve().parent))
from ftfy_fuzz import mojibake_docs  # noqa: E402
from ftfy_parity import read_corpus  # noqa: E402

from datatrove_rs import ftfy_check  # noqa: E402
from datatrove_rs.formatters import FTFYFormatter as Rs  # noqa: E402


def check_only() -> Rs:
    f = Rs()
    check, max_len = ftfy_check.build(f.config)

    def unchanged(text):
        return text if len(text) <= max_len and check.first_hit(text) is None else None

    f._rust = unchanged
    return f


def per_doc_us(make, texts, reps) -> float:
    times = []
    for _ in range(reps):
        step = make()
        docs = [Document(text=t, id=str(i)) for i, t in enumerate(texts)]
        s = time.perf_counter()
        for _ in step.run(iter(docs)):
            pass
        times.append((time.perf_counter() - s) / len(texts) * 1e6)
    return statistics.median(times)


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("corpus", nargs="+", type=Path)
    ap.add_argument("--n", type=int, default=2000)
    ap.add_argument("--reps", type=int, default=3)
    ap.add_argument("--mojibake", action="store_true", help="also time garbled copies (tools/ftfy_fuzz.py)")
    a = ap.parse_args()
    print(f"{'corpus':24} {'docs':>5} {'datatrove':>10} {'check':>9} {'port':>8} {'vs dt':>7} {'vs check':>8}  (us/doc)")
    sets = []
    for path in a.corpus:
        texts = read_corpus(path, a.n)
        sets.append((path.stem, texts))
        if a.mojibake:
            sets.append((path.stem + "+mojibake", mojibake_docs(texts[: len(texts) // 4], random.Random(0))))
    for name, texts in sets:
        dt, ck, rs = (per_doc_us(m, texts, a.reps) for m in (Py, check_only, Rs))
        print(f"{name:24} {len(texts):5} {dt:10.1f} {ck:9.1f} {rs:8.1f} {dt / rs:6.1f}x {ck / rs:7.1f}x", flush=True)


if __name__ == "__main__":
    main()
