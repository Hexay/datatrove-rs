"""Drop-in Rust implementations of datatrove steps; see README "Quickstart" for the ways to switch."""

from datatrove_rs import _mirror
from datatrove_rs._base import check_unicode_tables
from datatrove_rs._patching import is_patched, patch, unpatch
from datatrove_rs.filters import C4QualityFilter, FineWebQualityFilter, GopherQualityFilter, GopherRepetitionFilter

check_unicode_tables()
_mirror.install()

__all__ = [
    "C4QualityFilter",
    "FineWebQualityFilter",
    "GopherQualityFilter",
    "GopherRepetitionFilter",
    "MinhashDedupCluster",
    "MinhashDedupSignature",
    "SentenceDedupFilter",
    "SentenceDedupSignature",
    "is_patched",
    "patch",
    "unpatch",
]


def __getattr__(name):
    # datatrove.pipeline.dedup imports `tokenizers`, which filter-only installs may not have.
    if name in ("MinhashDedupSignature", "MinhashDedupCluster"):
        from datatrove_rs import minhash

        return getattr(minhash, name)
    if name in ("SentenceDedupSignature", "SentenceDedupFilter"):
        from datatrove_rs import sentence_dedup

        return getattr(sentence_dedup, name)
    raise AttributeError(f"module 'datatrove_rs' has no attribute {name!r}")
