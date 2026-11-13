"""The stats steps against datatrove: histogram keys, per-doc `extract_stats`, and whole runs split into tasks and
merged (`tools/stats_ref.py`). The large runs on FineWeb and FineWeb-2 are in docs/parity.md."""

import os
import pickle
import random
import sys
from fractions import Fraction

import pytest
from datatrove.data import Document
from datatrove.pipeline import stats as dt

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "tools"))
from datatrove_rs import _core  # noqa: E402
from datatrove_rs import stats as rs  # noqa: E402
from fuzz import fuzz_docs  # noqa: E402
from parity_ref import settle_is_alpha_state  # noqa: E402
from stats_parity import CASES, LANGUAGE_STEPS, encodable, spill_metadata  # noqa: E402
from stats_ref import case_fuzz, diff, make_metadata, run_tasks  # noqa: E402

DOCS = fuzz_docs(150, seed=3) + case_fuzz(150, seed=4)
STEPS = ["WordStats", "SentenceStats", "DocStats", "LineStats", "ParagraphStats"]


def test_histogram_key():
    rng = random.Random(0)
    values = [k / 2**e for e in range(1, 12) for k in range(0, 2**e + 1, max(1, 2**e // 64))]
    values += [rng.random() * 10 ** rng.randint(-8, 18) for _ in range(5000)]
    values += [float(Fraction(rng.randint(0, 10**6), rng.randint(1, 10**6))) for _ in range(5000)]
    for v in values:
        for d in (0, 1, 2, 3, 5, 8, 17):
            assert _core.stats_histogram_key(v, d) == str(round(v, d)), (v, d)
    for v in (0, 1, 12345, 2**53):
        assert _core.stats_histogram_key(v, 3) == str(round(v, 3))


@pytest.mark.parametrize("name", STEPS)
def test_extract_stats(name, tmp_path):
    settle_is_alpha_state()
    py, rust = getattr(dt, name)(str(tmp_path)), getattr(rs, name)(str(tmp_path))
    assert rust.rust() is not None
    for text in DOCS:
        a, b = (outcome(s, text) for s in (py, rust))
        assert a == b, text[:300]


def outcome(step, text):
    try:
        return repr(step.extract_stats(Document(text=text, id="0")))
    except Exception as e:  # noqa: BLE001 - datatrove's tokenizers raise on lone surrogates
        return type(e).__name__


@pytest.mark.parametrize("name,kwargs", CASES, ids=[f"{n}-{i}" for i, (n, _) in enumerate(CASES)])
def test_run_and_merge(name, kwargs):
    settle_is_alpha_state()
    docs = [d for d in DOCS if encodable(d)] if name in LANGUAGE_STEPS else DOCS
    meta = make_metadata(len(docs))
    problems = diff(*(run_tasks(getattr(m, name), kwargs, docs, meta, tasks=3) for m in (dt, rs)))
    assert not problems, problems[:3]


@pytest.mark.parametrize("meta", ["spill", "no_url"])
def test_python_takeover(meta):
    metadata = spill_metadata(make_metadata(len(DOCS))) if meta == "spill" else make_metadata(len(DOCS), url=False)
    ref, got = (run_tasks(m.DocStats, {}, DOCS, metadata, tasks=2) for m in (dt, rs))
    assert not diff(ref, got)
    assert (ref["error"] is not None) == (meta == "no_url")


def test_falls_back(tmp_path):
    assert rs.WordStats(str(tmp_path), language="kor").rust() is None
    assert rs.WordStats(str(tmp_path), short_word_max_chars_threshold=[3.0]).rust() is None
    assert rs.WordStats(str(tmp_path), stop_words="the and").rust() is None

    class Custom(rs.WordStats):
        def extract_stats(self, doc):
            return {"x": 1}

    assert not Custom(str(tmp_path))._can_run_fast()
    assert not rs.DocStats(str(tmp_path), histogram_round_digits=None)._can_run_fast()


def test_patch_and_mirror():
    import datatrove_rs
    from datatrove_rs.pipeline.stats import WordStats as Mirrored

    assert Mirrored is rs.WordStats
    datatrove_rs.patch()
    try:
        from datatrove.pipeline import stats

        assert stats.DocStats is rs.DocStats and stats.StatsMerger is dt.StatsMerger
    finally:
        datatrove_rs.unpatch()


def test_pickles_without_rust_handle(tmp_path):
    s = rs.WordStats(str(tmp_path))
    s.rust()
    t = pickle.loads(pickle.dumps(s))
    assert t._rust is None and t.rust() is not None
