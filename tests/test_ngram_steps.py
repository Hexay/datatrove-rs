"""SingleBloomFilter, NGramsDecontFilter/Indexer and UnigramLogProbFilter against datatrove (`tools/ngram_parity.py`).

Per-language runs are in test_languages.py, test_cjk.py and test_wordtok.py; the large runs are in docs/parity.md.
"""

import io
import os
import pickle
import sys
import tempfile

import pytest
from datatrove.data import Document
from datatrove.utils.word_tokenizers import SpaCyTokenizer

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "tools"))
from fuzz import fuzz_docs  # noqa: E402
from ngram_parity import (  # noqa: E402
    BLOOM_CASES,
    DECONT_CASES,
    check_bloom,
    check_decont,
    check_unigram,
    dtb,
    dtd,
    dtu,
    dup_docs,
    make_indexer,
    rsb,
    rsd,
    rsu,
    unigram_table,
)

DOCS = fuzz_docs(300, seed=3)


def _assert_clean(rows, report):
    assert [(n, bad, ok) for n, bad, ok in rows if bad or not ok] == [], report.getvalue()[:4000]


@pytest.mark.parametrize("name,cfg", BLOOM_CASES, ids=[c[0] for c in BLOOM_CASES])
def test_bloom_parity(name, cfg):
    report = io.StringIO()
    _assert_clean([(name, *check_bloom(name, cfg, DOCS + dup_docs(DOCS, 400), report))], report)


@pytest.mark.parametrize("name,cfg", DECONT_CASES, ids=[c[0] for c in DECONT_CASES])
def test_decont_parity(name, cfg):
    report = io.StringIO()
    _assert_clean([(name, *check_decont(name, cfg, DOCS, report))], report)


def test_unigram_parity():
    report = io.StringIO()
    with unigram_table(texts=DOCS):
        _assert_clean(check_unigram(DOCS, report, batch_sizes=(1, 3)), report)


def _bloom(cls, tmp, language="eng", **cfg):
    return cls(tmp, config=dtb.BloomFilterConfig(**({"m_bytes": 4095, "k": 3, "n_grams": 1} | cfg)), language=language)


def _step_outcome(step, text):
    try:
        r = step.step(Document(text=text, id="0"))
    except Exception as e:  # noqa: BLE001 — the exception type is part of the behavior under test
        r = type(e).__name__
    return r, bytes(step.bit_vector), step.total_shingles


@pytest.mark.parametrize("text", ["a b c", "a \ud800 b", "IS_ALPHA x y", ""])
@pytest.mark.parametrize("bits", [None, 1, 511])
def test_bloom_edge_cases_match(tmp_path, text, bits):
    """Lone surrogates, IS_ALPHA and a bit vector too short for the indexes (IndexError) run datatrove's code."""
    steps = [_bloom(cls, str(tmp_path)) for cls in (dtb.SingleBloomFilter, rsb.SingleBloomFilter)]
    for s in steps:
        if bits is not None:
            s.bit_vector = bytearray(bits)
    assert _step_outcome(steps[0], text) == _step_outcome(steps[1], text)


def test_bloom_fallbacks_and_pickling(tmp_path):
    class Custom(rsb.SingleBloomFilter):
        def get_shingles(self, text):
            return super().get_shingles(text.upper())

    assert _bloom(Custom, str(tmp_path)).rust() is None
    assert _bloom(rsb.SingleBloomFilter, str(tmp_path), n_grams=-1).rust() is None
    f = _bloom(rsb.SingleBloomFilter, str(tmp_path))
    f.step(Document(text="some words here", id="0"))
    g = pickle.loads(pickle.dumps(f))
    assert g._rust is None and g.bit_vector == f.bit_vector and g.rust() is not None


def test_decont_index_edits_match():
    """Tasks of None are skipped like datatrove; a replaced or grown index is reloaded."""
    config = dtd.NGramsDecontConfig(n_grams=2)
    ix = make_indexer(dtd.NGramsDecontIndexer, config, "eng")
    hashes = [h for d in DOCS[:40] for h in ix.compute_hashes(d)]
    with tempfile.TemporaryDirectory() as tmp:
        steps = [cls(tmp, config=config) for cls in (dtd.NGramsDecontFilter, rsd.NGramsDecontFilter)]
        for index in ({h: (None if i % 3 else "t:a") for i, h in enumerate(hashes)}, {h: "b" for h in hashes[::2]}):
            for grow in (False, True):
                if grow:
                    index[hashes[1]] = "grown"
                outs = []
                for s in steps:
                    s._index_hashes = index
                    docs = [Document(text=t, id="0") for t in DOCS[:60]]
                    outs.append([(s.filter(d), d.metadata) for d in docs])
                assert outs[0] == outs[1]
                assert any(r is not True for r, _ in outs[1])


def test_unigram_fallbacks():
    docs = [Document(text=t, id="0") for t in DOCS[:50]]
    with unigram_table(texts=DOCS):
        py, rs = dtu.UnigramLogProbFilter(), rsu.UnigramLogProbFilter()
        assert rs.rust() is not None
        for f in (py, rs):
            f.unigram_frequencies = dict(f.unigram_frequencies) | {"the": 0.0}
        assert [py.filter(d) for d in docs] == [rs.filter(d) for d in docs]
        zero = rsu.UnigramLogProbFilter()
        zero.unigram_frequencies["the"] = 0.0
        assert zero.rust() is None


def test_rust_paths_and_adoption():
    from datatrove_rs import pipeline  # noqa: F401 — mirror package
    from datatrove_rs.pipeline.decont import NGramsDecontFilter, NGramsDecontIndexer
    from datatrove_rs.pipeline.dedup import SingleBloomFilter

    assert (NGramsDecontFilter, NGramsDecontIndexer, SingleBloomFilter) == (
        rsd.NGramsDecontFilter,
        rsd.NGramsDecontIndexer,
        rsb.SingleBloomFilter,
    )
    custom = SpaCyTokenizer("en")
    custom.config = {"nlp": {"tokenizer": {"@tokenizers": "custom"}}}  # not a plain spacy.blank: no Rust tokenizer
    with tempfile.TemporaryDirectory() as tmp, unigram_table(texts=DOCS):
        for has_rust in (True, False):
            steps = [
                rsd.NGramsDecontFilter(tmp),
                make_indexer(rsd.NGramsDecontIndexer, dtd.NGramsDecontConfig(), "eng"),
                _bloom(rsb.SingleBloomFilter, tmp),
                rsu.UnigramLogProbFilter(),
            ]
            if not has_rust:
                for s in steps:
                    s.__dict__["tokenizer" if isinstance(s, rsd.NGramsDecontIndexer) else "language"] = custom
            assert [s.rust() is not None for s in steps] == [has_rust] * 4
