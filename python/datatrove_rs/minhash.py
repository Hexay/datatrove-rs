import numpy as np
from datatrove.pipeline.dedup import minhash as dt

from datatrove_rs import _core
from datatrove_rs._base import RustBacked


class MinhashDedupSignature(RustBacked, dt.MinhashDedupSignature):
    """Stage 1 only; the signature files it writes are consumed unchanged by datatrove's stages 2-4."""

    def _build_rust(self):
        a, b = self.parameters
        n = self.config.norm_config
        h = self.config.hash_config
        norm = (
            n.lowercase,
            n.norm_whitespace,
            n.remove_punctuation,
            n.norm_unicode_diacritics,
            n.norm_numbers,
            n.norm_weekdays,
            n.norm_monthnames,
        )
        return _core.MinHasher(
            self.config.n_grams, norm, h.hash_fc, h.precision, a.astype("<u8").tobytes(), b.astype("<u8").tobytes()
        )

    def get_shingles(self, text: str) -> np.ndarray:
        rs = self.rust_for(text)
        if not rs:
            return super().get_shingles(text)
        return np.frombuffer(rs.shingles(text), dtype="<u8").reshape((-1, 1))

    def get_signature(self, shingles: np.ndarray) -> list[list[int]]:
        rs = self.rust()
        if not rs:
            return super().get_signature(shingles)
        return rs.signature(np.ascontiguousarray(shingles, dtype="<u8").tobytes(), self.config.num_buckets)
