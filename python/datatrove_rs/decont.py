import numpy as np
from datatrove.pipeline.decont import n_grams as dt

from datatrove_rs import _core
from datatrove_rs._base import _NOT_APPLICABLE, FALLBACK, RustBacked, norm_fields
from datatrove_rs.filters import _RustFilter


def _build(config, language):
    if not (type(config.n_grams) is int and config.n_grams >= 0):
        return _NOT_APPLICABLE
    h = config.hash_config
    return _core.NGramsDecont(config.n_grams, norm_fields(config.norm_config), h.hash_fc, h.precision, language)


class _Indexed:
    """The Rust lookup plus the `_index_hashes` dict it was loaded from; reloaded when that dict changes."""

    __slots__ = ("rs", "source", "size")

    def __init__(self, rs):
        self.rs, self.source, self.size = rs, None, -1

    def find(self, text: str, index: dict):
        """`(n_gram, hash)` of the first indexed n-gram, or False when `index` can't be mirrored in Rust."""
        if index is not self.source or len(index) != self.size:
            # datatrove skips n-grams whose task is None, and only int keys can equal a hash.
            keys = [k for k, task in index.items() if task is not None]
            if not all(type(k) is int and 0 <= k < 1 << 64 for k in keys):
                return False
            self.rs.set_index(np.array(keys, dtype="<u8").tobytes())
            self.source, self.size = index, len(index)
        return self.rs.find(text)


class NGramsDecontFilter(_RustFilter, dt.NGramsDecontFilter):
    """n-gram hashing and the index lookup in Rust (`crates/datatrove-core/src/decont.rs`); the index is still
    loaded by datatrove's `load_index_hashes`, and metadata and stats are set as datatrove does."""

    def _build_rust(self, language):
        rs = _build(self.config, language)
        return _Indexed(rs) if rs else _NOT_APPLICABLE

    def _rust_filter(self, rs, doc, counters=None):
        if self._index_hashes is None:
            self.load_index_hashes()
        found = rs.find(doc.text, self._index_hashes)
        if found is False:
            return dt.NGramsDecontFilter.filter(self, doc)
        if found is None:
            return True
        n_gram, h = found
        task = self._index_hashes[h]
        doc.metadata["contaminated_ngram"] = n_gram
        doc.metadata["contaminated_task"] = task
        add = counters.add if counters else self.stat_update
        add(f"contaminated_{task}")
        if ":" in task:
            add(f"contaminated_tg_{task[: task.index(':')]}")
        return False, "contaminated"


class NGramsDecontIndexer(RustBacked, dt.NGramsDecontIndexer):
    """`compute_hashes` in Rust; reading tasks and writing the index files stay datatrove's."""

    @property
    def language(self):
        return self.tokenizer

    def _build_rust(self, language):
        return _build(self.config, language)

    def compute_hashes(self, label: str, query: str | None = None) -> list[int]:
        rs = self.rust_for(label if query is None else label + query)
        if rs:
            c = self.config
            try:
                return rs.compute_hashes(label, query, bool(c.find_query_ngrams), bool(c.find_overlap_ngrams))
            except FALLBACK:
                pass
        return super().compute_hashes(label, query)
