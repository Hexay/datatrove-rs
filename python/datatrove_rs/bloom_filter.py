from datatrove.pipeline.dedup import bloom_filter as dt
from datatrove.utils.typeshelper import StatHints

from datatrove_rs import _core
from datatrove_rs._base import _NOT_APPLICABLE, FALLBACK, RustBacked, norm_fields

# Overriding any of these in a subclass changes what `step` computes, so it then runs datatrove's `step`.
_STEP_PARTS = ("step", "get_shingles", "get_indexes", "query", "update_bf", "parameters")


def _is_count(x, limit: int = 1 << 63) -> bool:
    return type(x) is int and 0 <= x < limit


class SingleBloomFilter(RustBacked, dt.SingleBloomFilter):
    """`step` in Rust (`crates/datatrove-core/src/bloom.rs`). The bit vector stays the step's `bytearray`, updated
    in place, so `run()`, its stats and the saved `bloom_filter.bloom` are datatrove's own."""

    def _build_rust(self, language):
        cls, c = type(self), self.config
        if any(getattr(cls, p) is not getattr(SingleBloomFilter, p) for p in _STEP_PARTS):
            return _NOT_APPLICABLE
        if not (_is_count(c.n_grams) and _is_count(c.k) and _is_count(c.m_bytes)):
            return _NOT_APPLICABLE
        a, b = self.parameters
        h = c.hash_config
        return _core.BloomFilter(
            c.n_grams,
            norm_fields(c.norm_config),
            h.hash_fc,
            h.precision,
            a.astype("<u8").tobytes(),
            b.astype("<u8").tobytes(),
            c.m_bytes,
            language,
        )

    def step(self, doc) -> bool:
        rs = self.rust_for(doc.text)
        out = None
        if rs and type(self.bit_vector) is bytearray:
            try:
                out = rs.step(doc.text, self.bit_vector)
            except FALLBACK:
                pass
        if out is None:
            return super().step(doc)
        n, dups = out
        self.total_shingles += n
        if n == 0:
            return True
        if dups / n > self.config.duplicate_threshold:
            self.stat_update(StatHints.dropped)
            return False
        return True
