"""`FastRun.run()` against datatrove's `BaseFilter.run()`: outputs, live stats, exclusion writer, batching, fallbacks."""

import os
import random
import sys
from itertools import islice

import pytest
from datatrove.data import Document
from datatrove.pipeline import filters as dt
from datatrove.utils.stats import MetricStats, MetricStatsDict

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "tools"))
import datatrove_rs as rs  # noqa: E402
from datatrove_rs._run import Counters  # noqa: E402
from fuzz import fuzz_docs  # noqa: E402
from parity_ref import RecordingWriter, make_step, settle_is_alpha_state, stats_of  # noqa: E402

settle_is_alpha_state()
DOCS = [t for t in fuzz_docs(300, seed=3) if t.strip()][:200]
PAIRS = [
    (dt.GopherRepetitionFilter, rs.GopherRepetitionFilter),
    (dt.GopherQualityFilter, rs.GopherQualityFilter),
    (dt.C4QualityFilter, rs.C4QualityFilter),
    (dt.FineWebQualityFilter, rs.FineWebQualityFilter),
]


def _docs(texts):
    return (Document(text=t, id=str(i), metadata={"token_count": i % 3}) for i, t in enumerate(texts))


def _live_trace(step, texts):
    """Stats snapshot after every yielded doc, then the final ones: stats must match while the run is suspended too."""
    trace = [(d.id, d.text, sorted(d.metadata.items()), stats_of(step)) for d in step.run(_docs(texts), rank=1)]
    return trace, stats_of(step)


def _safe_texts(py_cls, kwargs):
    step = py_cls(**kwargs)
    out = []
    for t in DOCS:
        try:
            step.filter(Document(text=t, id="0"))
        except ZeroDivisionError:
            continue
        out.append(t)
    return out


@pytest.mark.parametrize("batch_size", [1, 3])
@pytest.mark.parametrize("writer", [False, True])
@pytest.mark.parametrize("py_cls,rs_cls", PAIRS, ids=[p[1].__name__ for p in PAIRS])
def test_run_matches_datatrove(py_cls, rs_cls, writer, batch_size):
    texts = _safe_texts(py_cls, {})
    kwargs = {"batch_size": batch_size}
    steps = [make_step(cls, kwargs, exclusion_writer=RecordingWriter() if writer else None) for cls in (py_cls, rs_cls)]
    assert _live_trace(steps[0], texts) == _live_trace(steps[1], texts)
    if writer:
        assert steps[0].exclusion_writer.log == steps[1].exclusion_writer.log


def test_fast_path_is_taken():
    step = make_step(rs.C4QualityFilter, {"batch_size": 2})
    assert step._can_run_fast(step.rust())


def test_run_twice_continues_stats():
    a, b = dt.C4QualityFilter(), rs.C4QualityFilter()
    for step in (a, b):
        for chunk in (DOCS[:50], DOCS[50:120]):
            list(step.run(_docs(chunk)))
    assert stats_of(a) == stats_of(b)


def test_early_close_exits_writer():
    logs = []
    for cls in (dt.GopherQualityFilter, rs.GopherQualityFilter):
        step = cls(exclusion_writer=RecordingWriter())
        gen = step.run(_docs(DOCS))
        list(islice(gen, 5))
        gen.close()
        logs.append((step.exclusion_writer.log, stats_of(step)))
    assert logs[0] == logs[1] and logs[0][0][-1] == "exit"


class DropAll(rs.GopherQualityFilter):
    def filter(self, doc):
        return False, "custom"


def test_overridden_filter_falls_back_to_datatrove_run():
    step = DropAll()
    assert not step._can_run_fast(step.rust())
    assert list(step.run(_docs(DOCS[:10]))) == []
    assert step.stats["dropped_custom"].n == 10


def test_non_english_runs_python_path():
    texts = DOCS[:20]
    a, b = dt.GopherQualityFilter(language="fra"), rs.GopherQualityFilter(language="fra")
    assert not b._can_run_fast(b.rust())
    assert _live_trace(a, texts) == _live_trace(b, texts)


def test_bad_batch_size_raises_like_datatrove():
    with pytest.raises(ValueError):
        list(make_step(rs.GopherQualityFilter, {"batch_size": 0}).run(_docs(DOCS[:3])))


@pytest.mark.parametrize(
    "start",
    [None, MetricStats(), MetricStats(total=5, n=5, mean=1.0, min=1, max=1), MetricStats(_running_variance=-0.0),
     MetricStats(total=2.5, n=2, mean=1.25, min=0.5, max=2), MetricStats.from_dict(7), MetricStats.from_dict(1.0),
     MetricStats(total=3, n=3, mean=1.0, min=1, max=1, _running_variance=-0.0)],
)
def test_counters_are_bit_exact(start):
    rng = random.Random(0)
    ref, got = MetricStatsDict(), MetricStatsDict()
    if start is not None:
        ref["x"], got["x"] = (MetricStats(**vars(start)) for _ in range(2))
    counters = Counters(got)
    for _ in range(30):
        k = rng.randint(1, 4)
        for _ in range(k):
            ref["x"].update(1)
        counters.add("x", k)
        assert repr(vars(ref["x"])) == repr(vars(got["x"]))
