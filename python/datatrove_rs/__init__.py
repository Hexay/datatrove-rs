"""Drop-in Rust implementations of datatrove's English Gopher, C4, FineWeb filters and MinHash stage 1."""

from datatrove_rs._base import check_unicode_tables
from datatrove_rs.filters import C4QualityFilter, FineWebQualityFilter, GopherQualityFilter, GopherRepetitionFilter

check_unicode_tables()

__all__ = [
    "C4QualityFilter",
    "FineWebQualityFilter",
    "GopherQualityFilter",
    "GopherRepetitionFilter",
    "MinhashDedupSignature",
]


def __getattr__(name):
    # datatrove.pipeline.dedup imports `tokenizers`, which filter-only installs may not have.
    if name == "MinhashDedupSignature":
        from datatrove_rs.minhash import MinhashDedupSignature

        return MinhashDedupSignature
    raise AttributeError(f"module 'datatrove_rs' has no attribute {name!r}")
