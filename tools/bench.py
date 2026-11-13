"""Throughput of datatrove's Python steps vs datatrove_rs, single thread, same docs.

Usage: bench.py <fineweb.parquet> [n_docs=1500] [reps=3] [language]

Filters are timed via `filter()` on fresh Documents; the chain runs GopherRepetition -> GopherQuality ->
C4Quality -> FineWebQuality through `run()` as a FineWeb-style pipeline would. MinHash times
get_shingles + get_signature; the Bloom filter `step()` on a fresh 1 MB filter per rep. Sentence dedup times stage 1
and stage 3 `run()` (default SentDedupConfig; stage 3 on the docs datatrove's `span_tokenize` doesn't raise on). Reports the median of `reps`.
"""

import os
import random
import statistics
import sys
import time

import numpy as np
import pyarrow.parquet as pq
from datatrove.data import Document
from datatrove.pipeline import filters as dt
from datatrove.pipeline import stats as dts
from datatrove.pipeline.dedup import minhash as dtm
from datatrove.pipeline.dedup import sentence_dedup as dtsd
from loguru import logger
from sentdedup_ref import span_safe

import datatrove_rs as rs
from datatrove_rs import stats as rss
from datatrove_rs.minhash import MinhashDedupSignature as RsMinhash
from datatrove_rs.sentence_dedup import SentenceDedupFilter as RsSentFilter
from datatrove_rs.sentence_dedup import SentenceDedupSignature as RsSentSig

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


LANG = {"language": sys.argv[4]} if len(sys.argv) > 4 else {}
STATS = ("WordStats", "SentenceStats", "DocStats", "LineStats", "ParagraphStats")
ONLY = os.environ.get("BENCH_ONLY")  # e.g. "Stats": only rows whose name contains it


def filter_all(cls, texts):
    step = cls(**LANG)
    return lambda: [step.filter(Document(text=t, id="0")) for t in texts]


def chain(classes, texts):
    def run():
        docs = (Document(text=t, id=str(i)) for i, t in enumerate(texts))
        for cls in classes:
            docs = cls(**LANG).run(docs, rank=0)
        return sum(1 for _ in docs)

    return run


def minhash(cls, texts, tmp):
    step = cls(tmp, **LANG)

    def run():
        for t in texts:
            s = step.get_shingles(t)
            if s.size:
                step.get_signature(s)

    return run


def stats_run(cls, texts, tmp):
    """A stats step's full `run()` (all four groups, files written) over docs with URLs."""
    kwargs = LANG if cls.__name__ in ("WordStats", "SentenceStats") else {}

    def run():
        docs = (Document(text=t, id=str(i), metadata={"url": f"https://www.site{i % 50}.example.com/{i}"})
                for i, t in enumerate(texts))
        return sum(1 for _ in cls(tmp, **kwargs).run(docs))

    return run


def ngram_steps(texts, tmp):
    """(name, py_fn, rs_fn) for SingleBloomFilter.step, NGramsDecontFilter and UnigramLogProbFilter `filter()`.
    The decont index holds n-grams of every 5th doc; unigram reads datatrove's table (downloaded if not cached)."""
    sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
    from ngram_parity import decont_pairs, dtb, dtd, dtu, rsb, rsd, rsu, write_index

    lang = LANG.get("language", "eng")
    index = os.path.join(tmp, "decont-index")
    os.makedirs(index, exist_ok=True)
    write_index(index, decont_pairs(texts), dtd.NGramsDecontConfig(), lang)

    def bloom(cls):
        def run():
            step = cls(tmp, config=dtb.BloomFilterConfig(m_bytes=(1 << 20) - 1, k=5), language=lang)
            for t in texts:
                step.step(Document(text=t, id="0"))

        return run

    def decont(cls):
        step = cls(index, language=lang)
        return lambda: [step.filter(Document(text=t, id="0")) for t in texts]

    return [
        ("SingleBloomFilter", bloom(dtb.SingleBloomFilter), bloom(rsb.SingleBloomFilter)),
        ("NGramsDecontFilter", decont(dtd.NGramsDecontFilter), decont(rsd.NGramsDecontFilter)),
        ("UnigramLogProbFilter", filter_all(dtu.UnigramLogProbFilter, texts), filter_all(rsu.UnigramLogProbFilter, texts)),
    ]


def _docs(texts):
    return (Document(text=t, id=str(i)) for i, t in enumerate(texts))


def sentdedup_stage1(cls, texts, tmp):
    return lambda: cls(os.path.join(tmp, "sigs"), config=dtsd.SentDedupConfig(), **LANG).run(_docs(texts))


def sentdedup_stage3(cls, texts, tmp):
    """`run()` with one duplicate 3-sentence span per doc (at sentence 0-2), so every doc goes through
    `remove_dup_sentences` and the min_doc_words / min_num_sentences checks."""
    folder = os.path.join(tmp, "dups")
    os.makedirs(os.path.join(folder, "0000"), exist_ok=True)
    rng = random.Random(0)
    dups = np.array([(i, rng.randrange(3)) for i in range(len(texts))], dtype=[("doc", "<u4"), ("sent", "<u2")])
    dups.tofile(os.path.join(folder, "0000", "00000.c4_dup"))
    return lambda: sum(1 for _ in cls(folder, config=dtsd.SentDedupConfig(), **LANG).run(_docs(texts)))


def c4_ok(texts):
    """The docs datatrove's own C4 doesn't raise on (some Indic sentence splits do, Urdu without urduhack)."""
    step = dt.C4QualityFilter(**LANG)

    def ok(t):
        try:
            step.filter(Document(text=t, id="0"))
            return True
        except Exception:
            return False

    return [t for t in texts if ok(t)]


def main():
    path = sys.argv[1]
    n = int(sys.argv[2]) if len(sys.argv) > 2 else 1500
    reps = int(sys.argv[3]) if len(sys.argv) > 3 else 3
    texts = pq.read_table(path, columns=["text"]).slice(0, n).column("text").to_pylist()
    c4_texts = c4_ok(texts)
    mb = sum(len(t.encode()) for t in texts) / 1e6
    tmp = os.path.join(os.environ.get("TEMP", "/tmp"), "datatrove_rs_bench")
    rows = []
    for name, p, r in STEPS:
        t = c4_texts if name == "C4Quality" else texts
        rows.append((name, t, filter_all(p, t), filter_all(r, t)))
    py_chain, rs_chain = (chain([s[i] for s in STEPS], c4_texts) for i in (1, 2))
    rows.append(("Chain (run, 4 filters)", c4_texts, py_chain, rs_chain))
    rows.append(("MinHash stage 1", texts, minhash(dtm.MinhashDedupSignature, texts, tmp), minhash(RsMinhash, texts, tmp)))
    for name in STATS:
        rows.append((f"{name} (run)", texts, stats_run(getattr(dts, name), texts, tmp), stats_run(getattr(rss, name), texts, tmp)))
    rows += [(name, texts, p, r) for name, p, r in ngram_steps(texts, tmp)]
    s1 = sentdedup_stage1(dtsd.SentenceDedupSignature, texts, tmp), sentdedup_stage1(RsSentSig, texts, tmp)
    rows.append(("SentenceDedup stage 1 (run)", texts, *s1))
    sd_texts = span_safe(texts, LANG.get("language", "eng"))
    s3 = sentdedup_stage3(dtsd.SentenceDedupFilter, sd_texts, tmp), sentdedup_stage3(RsSentFilter, sd_texts, tmp)
    rows.append(("SentenceDedup stage 3 (run, 1 dup/doc)", sd_texts, *s3))

    print(f"{n} docs, {mb:.1f} MB, median of {reps}, language {LANG.get('language', 'default')}")
    print(f"C4 and the chain run on the {len(c4_texts)} docs datatrove's C4 doesn't raise on\n")
    print(f"| Step | datatrove docs/s | datatrove_rs docs/s | speedup |\n|---|---|---|---|")
    logger.remove()
    for name, t, py_fn, rs_fn in rows:
        if ONLY and ONLY not in name:
            continue
        if not t:
            print(f"| {name} | - | - | - |")
            continue
        py_fn(), rs_fn()  # warm-up: tokenizer construction, spaCy load
        tp, tr = timed(py_fn, reps), timed(rs_fn, reps)
        print(f"| {name} | {len(t) / tp:,.0f} | {len(t) / tr:,.0f} | {tp / tr:.1f}x |")


if __name__ == "__main__":
    main()
