import math

import numpy as np
from datatrove.pipeline.filters import unigram_log_probs as dt

from datatrove_rs import _core
from datatrove_rs._base import _NOT_APPLICABLE
from datatrove_rs.filters import _RustFilter

_MISSING = 1e-9  # datatrove's frequency for words not in the table


class UnigramLogProbFilter(_RustFilter, dt.UnigramLogProbFilter):
    """Words, lookups and the mean in Rust (`crates/datatrove-core/src/filters/unigram.rs`). Each frequency's log
    comes from the same scalar `np.log` call datatrove makes, so the sums and the threshold comparison are exact."""

    def _build_rust(self, language):
        if type(self).get_logprob is not dt.UnigramLogProbFilter.get_logprob:
            return _NOT_APPLICABLE
        freqs = self.unigram_frequencies
        if type(freqs) is not dict or not all(type(w) is str and type(f) is float for w, f in freqs.items()):
            return _NOT_APPLICABLE
        with np.errstate(divide="ignore", invalid="ignore"):
            log_of = {f: np.log(f) for f in set(freqs.values())}
            log_of[_MISSING] = np.log(_MISSING)
        # A zero or negative frequency would make numpy warn on every doc; leave those tables to datatrove.
        if not all(math.isfinite(v) for v in log_of.values()):
            return _NOT_APPLICABLE
        logs = [float(log_of[f]) for f in freqs.values()]
        rs = _core.UnigramLogProb(list(freqs), logs, float(log_of[_MISSING]), language)
        return _Unigram(rs, freqs)

    def _rust_filter(self, rs, doc, counters=None):
        if rs.source is not self.unigram_frequencies:
            return dt.UnigramLogProbFilter.filter(self, doc)
        mean = rs.rs.mean_logprob(doc.text)
        return (0 if mean is None else np.float64(mean)) > self.logprobs_threshold


class _Unigram:
    __slots__ = ("rs", "source")

    def __init__(self, rs, source: dict):
        self.rs, self.source = rs, source
