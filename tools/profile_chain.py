"""cProfile of the 4-filter datatrove_rs chain through `run()`, to see the Python plumbing cost.

Usage: profile_chain.py <fineweb.parquet> [n_docs=1000] [top=25]
"""

import cProfile
import pstats
import sys

import pyarrow.parquet as pq
from datatrove.data import Document

import datatrove_rs as rs

CHAIN = [rs.GopherRepetitionFilter, rs.GopherQualityFilter, rs.C4QualityFilter, rs.FineWebQualityFilter]


def run_chain(texts):
    docs = (Document(text=t, id=str(i)) for i, t in enumerate(texts))
    for cls in CHAIN:
        docs = cls().run(docs, rank=0)
    return sum(1 for _ in docs)


def main():
    n = int(sys.argv[2]) if len(sys.argv) > 2 else 1000
    top = int(sys.argv[3]) if len(sys.argv) > 3 else 25
    texts = pq.read_table(sys.argv[1], columns=["text"]).slice(0, n).column("text").to_pylist()
    run_chain(texts)  # warm-up: tokenizer construction
    prof = cProfile.Profile()
    prof.runcall(run_chain, texts)
    pstats.Stats(prof).sort_stats("tottime").print_stats(top)


if __name__ == "__main__":
    main()
