"""The single map from datatrove classes to their Rust-backed subclasses; `patch` and the mirror both read it."""

import importlib.util
from functools import lru_cache


@lru_cache(maxsize=None)
def replacements() -> dict[type, type]:
    """{datatrove class: datatrove_rs subclass}. MinHash and the Bloom filter are skipped when
    `datatrove.pipeline.dedup` can't import (it needs `tokenizers`, which filter-only installs may lack)."""
    from datatrove_rs import filters as rs
    from datatrove_rs.badwords import C4BadWordsFilter
    from datatrove_rs.decont import NGramsDecontFilter, NGramsDecontIndexer
    from datatrove_rs.formatters import FTFYFormatter, PIIFormatter
    from datatrove_rs.language_filter import LanguageFilter
    from datatrove_rs.readers import WarcReader
    from datatrove_rs.unigram import UnigramLogProbFilter
    from datatrove_rs.url_filter import URLFilter

    out = {
        _datatrove_base(cls): cls
        for cls in (
            rs.GopherRepetitionFilter,
            rs.GopherQualityFilter,
            rs.C4QualityFilter,
            rs.FineWebQualityFilter,
            WarcReader,
            URLFilter,
            LanguageFilter,
            PIIFormatter,
            FTFYFormatter,
            C4BadWordsFilter,
            UnigramLogProbFilter,
            NGramsDecontFilter,
            NGramsDecontIndexer,
        )
    }
    try:
        from datatrove_rs import stats

        for cls in (stats.WordStats, stats.SentenceStats, stats.DocStats, stats.LineStats, stats.ParagraphStats):
            out[_datatrove_base(cls)] = cls
    except ImportError:
        pass
    try:
        from datatrove_rs.bloom_filter import SingleBloomFilter
        from datatrove_rs.minhash import MinhashDedupCluster, MinhashDedupSignature

        for cls in (MinhashDedupSignature, MinhashDedupCluster, SingleBloomFilter):
            out[_datatrove_base(cls)] = cls
    except ImportError:
        pass
    if importlib.util.find_spec("trafilatura_rs") is not None:
        from datatrove_rs.extractors import Trafilatura

        out[_datatrove_base(Trafilatura)] = Trafilatura
    return out


def _datatrove_base(cls: type) -> type:
    """The datatrove class `cls` replaces: its first base defined inside datatrove."""
    return next(b for b in cls.__mro__[1:] if b.__module__.startswith("datatrove."))
