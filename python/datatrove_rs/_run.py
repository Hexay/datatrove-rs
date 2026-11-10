"""A cheaper `BaseFilter.run()` for the Rust-backed filters, with identical outputs and stats.

The loop is `BaseFilter.run()` inlined: same batching, same order of filter calls, stat updates, yields and
exclusion-writer calls. Only the stat bookkeeping is cheaper, and exact: see `Counters`.
"""

import contextlib
from itertools import islice

from datatrove.pipeline.filters.base_filter import BaseFilter
from datatrove.utils.typeshelper import StatHints

# BaseFilter methods `FastRun.run` inlines; a subclass overriding any of them (or `filter`) gets `BaseFilter.run`.
_INLINED = ("filter_batch", "stat_update", "update_doc_stats", "track_time")


def _is_fixed_point(s) -> bool:
    """True when `s.update(1)` would only add 1 to `total` and `n`: Welford with x == mean == 1.0 adds 0.0."""
    return s.n >= 2 and type(s.total) is int and type(s.mean) is float and s.mean == 1.0 and s.min <= 1 <= s.max


class Counters:
    """`stat_update(label)` (value 1, no unit) on a `MetricStatsDict`, bit-for-bit, without a Welford step per call.

    A label first goes through real `MetricStats.update(1)` calls until it reaches the fixed point above (n >= 2, so
    the last real call already applied `_running_variance += 0.0`, which could otherwise flip -0.0). From then on
    `update(1)` changes only total and n, so those are incremented directly. Labels are inserted into the dict at the
    same moment `stat_update` would insert them, so key order matches too. Assumes nothing else feeds these labels
    values other than 1, which holds for every counter datatrove's filters emit.
    """

    __slots__ = ("stats", "_hot")

    def __init__(self, stats):
        self.stats = stats
        self._hot = {}

    def add(self, label: str, k: int = 1):
        s = self._hot.get(label)
        if s is None:
            s = self.stats[label]
            while k and label not in self._hot:
                s.update(1)
                k -= 1
                if _is_fixed_point(s):
                    self._hot[label] = s
        if k:
            s.total += k
            s.n += k


class FastRun:
    """Mixin for `BaseFilter` subclasses; the class providing `_doc_filter` is the one whose behaviour is inlined."""

    def _doc_filter(self, rs, counters):
        """A callable doc -> filter result, equivalent to `self.filter`, updating stats through `counters`."""
        raise NotImplementedError

    def _can_run_fast(self, rs) -> bool:
        cls = type(self)
        owner = next(c for c in cls.__mro__ if "_doc_filter" in c.__dict__)
        batch_size = self.batch_size
        return (
            rs is not None
            and type(batch_size) is int
            and batch_size >= 1
            and cls.filter is owner.filter
            and all(getattr(cls, name) is getattr(BaseFilter, name) for name in _INLINED)
        )

    def run(self, data, rank: int = 0, world_size: int = 1):
        rs = self.rust()
        if not self._can_run_fast(rs):
            yield from super().run(data, rank, world_size)
            return
        counters = Counters(self.stats.stats)
        filter_doc = self._doc_filter(rs, counters)
        timer = self.stats.time_stats
        batch_size = self.batch_size
        with self.exclusion_writer if self.exclusion_writer else contextlib.nullcontext() as writer:
            if batch_size == 1:
                for doc in data:
                    with timer:
                        result = filter_doc(doc)
                    if self._account(doc, result, counters, writer, rank):
                        yield doc
                return
            it = iter(data)
            while batch := list(islice(it, batch_size)):
                counters.add("batches")
                timer.unit = "batch"
                with timer:
                    results = list(map(filter_doc, batch))
                for doc, result in zip(batch, results):
                    if self._account(doc, result, counters, writer, rank):
                        yield doc

    def _account(self, doc, result, counters, writer, rank) -> bool:
        """`BaseFilter.run`'s per-doc stats and exclusion writing; returns whether to yield `doc`."""
        counters.add(StatHints.total)
        reason = None
        if isinstance(result, tuple):
            result, reason = result
        if result:
            counters.add(StatHints.forwarded)
            stats = counters.stats
            stats["doc_len"].update(len(doc.text), "doc")
            if token_count := doc.metadata.get("token_count", None):
                stats["doc_len_tokens"].update(token_count, "doc")
            return True
        counters.add(StatHints.dropped)
        if reason:
            counters.add(f"dropped_{reason}")
        if self.exclusion_writer:
            if reason:
                doc.metadata["filter_reason"] = reason
            writer.write(doc, rank)
        return False
