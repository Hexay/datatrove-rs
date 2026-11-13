"""`BaseStats.run` with its per-group bookkeeping in Rust (`_core.StatsAccumulator`): same files, metadata and stats.

The loop is `BaseStats.run` inlined: per doc `extract_stats`, `get_kv`'s fqdn/suffix metadata writes in group order,
the counters, then `doc.metadata.update`. `StatsAccumulator.add` reproduces `counters[stat][key] += value`
(`MetricStats.__add__` with `MetricStats.from_dict(value)`) operation for operation on ints and floats. On any other
value, key or token count it records nothing, its counters become real `MetricStats` and datatrove's loop takes
over for the rest of the run. The JSON files come from datatrove's save loop (inlined) on real `MetricStats`.
"""

import heapq
import importlib.metadata
import json
from collections import defaultdict
from functools import lru_cache

from datatrove.pipeline.base import PipelineStep
from datatrove.pipeline.stats.base import BaseStats
from datatrove.utils.stats import MetricStats, MetricStatsDict
from loguru import logger

from datatrove_rs import _core

# datatrove releases whose stats steps, `BaseStats.run` and `MetricStats` the Rust side reproduces.
VERIFIED_DATATROVE = frozenset({"0.10.1"})
_GROUPS = ("summary", "histogram", "fqdn", "suffix")
_KEYED = ("fqdn", "suffix")


@lru_cache(maxsize=1)
def datatrove_verified() -> bool:
    return importlib.metadata.version("datatrove") in VERIFIED_DATATROVE


class FastStatsRun:
    """Mixin for the Rust-backed `BaseStats` subclasses (each defines `extract_stats`)."""

    def _can_run_fast(self) -> bool:
        cls = type(self)
        owner = next(c for c in cls.__mro__ if c.__module__ == "datatrove_rs.stats")
        digits = self.histogram_round_digits
        return (
            datatrove_verified()
            and cls.extract_stats is owner.extract_stats
            and cls.get_kv is BaseStats.get_kv
            and cls.track_time is PipelineStep.track_time
            and type(digits) is int
            and 0 <= digits <= 1000
            and all(type(g) is str and g in _GROUPS for g in self.groups)
        )

    def run(self, data, rank: int = 0, world_size: int = 1):
        if not self._can_run_fast():
            yield from super().run(data, rank, world_size)
            return
        groups = list(dict.fromkeys(self.groups))
        keyed = [g for g in groups if g in _KEYED]
        acc = _core.StatsAccumulator(groups, self.histogram_round_digits)
        counters = None  # datatrove's `groups_dicts`, once the run left Rust
        timer = self.stats.time_stats
        for doc in data:
            with timer:
                try:
                    doc_stats = self.extract_stats(doc)
                except Exception as e:
                    logger.error(f"Error while extracting stats from document {doc.id}", exc_info=e)
                    raise e
                if counters is None and doc_stats:
                    for group in keyed:
                        self.get_kv(doc, None, group)
                    if not acc.add(doc_stats, doc.metadata, len(doc.text)):
                        counters = _drain(acc, groups)
                if counters is not None:
                    self._count(doc, doc_stats, counters)
                doc.metadata.update(doc_stats)
            yield doc
        self._save(counters if counters is not None else _drain(acc, groups), rank)

    def _count(self, doc, doc_stats, groups_dicts):
        """`BaseStats.run`'s per-doc counter loop."""
        for group, counters in groups_dicts.items():
            for stat, value in doc_stats.items():
                key, value = self.get_kv(doc, value, group)
                if not isinstance(value, dict):
                    counters[stat][key] += value
                else:
                    for suffix, val in value.items():
                        stat_name = stat if not suffix else f"{stat}__{suffix}"
                        counters[stat_name][key] += val

    def _save(self, groups_dicts, rank):
        """`BaseStats.run`'s save loop, writing each file in one call."""
        for group, stats_dict in groups_dicts.items():
            group_top_k_keys = None

            for stat_name, stat_values in stats_dict.items():
                if group in self.top_k_cfg.top_k_groups:
                    if group_top_k_keys is None:
                        group_top_k_keys = heapq.nlargest(
                            self.top_k_cfg.top_k, stat_values, key=lambda x: stat_values[x].n
                        )

                    stat_values = MetricStatsDict(init={s: stat_values[s] for s in group_top_k_keys})

                # json.dumps: the same text as json.dump, which streams it to fsspec in thousands of tiny writes
                with self.output_folder.open(f"{group}/{stat_name}/{rank:05d}.json", "wt") as f:
                    f.write(json.dumps(stat_values.to_dict()))


def _drain(acc, groups):
    """The accumulator's counters as datatrove's `groups_dicts` of `MetricStats` (unit "task": every one was built
    by `+= value`)."""
    groups_dicts = {group: defaultdict(MetricStatsDict) for group in groups}
    for group, tables in acc.drain():
        stats_dict = groups_dicts[group]
        for stat_name, rows in tables:
            counters = stats_dict[stat_name]
            for key, total, n, mean, lo, hi, running_variance in rows:
                counters[key] = MetricStats(total, n, mean, lo, hi, running_variance, "task")
    return groups_dicts
