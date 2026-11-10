"""The single map from datatrove classes to their Rust-backed subclasses; `patch` and the mirror both read it."""

import importlib.util
from functools import lru_cache


@lru_cache(maxsize=None)
def replacements() -> dict[type, type]:
    """{datatrove class: datatrove_rs subclass}. MinHash is skipped when `datatrove.pipeline.dedup` can't import
    (it needs `tokenizers`, which filter-only installs may lack)."""
    from datatrove_rs import filters as rs

    out = {
        _datatrove_base(cls): cls
        for cls in (rs.GopherRepetitionFilter, rs.GopherQualityFilter, rs.C4QualityFilter, rs.FineWebQualityFilter)
    }
    try:
        from datatrove_rs.minhash import MinhashDedupSignature

        out[_datatrove_base(MinhashDedupSignature)] = MinhashDedupSignature
    except ImportError:
        pass
    if importlib.util.find_spec("trafex") is not None:
        from datatrove_rs.extractors import Trafilatura

        out[_datatrove_base(Trafilatura)] = Trafilatura
    return out


def _datatrove_base(cls: type) -> type:
    """The datatrove class `cls` replaces: its first base defined inside datatrove."""
    return next(b for b in cls.__mro__[1:] if b.__module__.startswith("datatrove."))
