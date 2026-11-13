"""Chinese and Japanese: datatrove's `SpaCyTokenizer("zh")` segments with jieba and `SpaCyTokenizer("ja")` with
SudachiPy. Rust reproduces both from the same data, read from the installed packages when first needed; without
them (or with other versions) these languages run datatrove's Python code. See docs/parity.md."""

from functools import lru_cache

from datatrove_rs import _core

JIEBA_VERSION = "0.42.1"

# The configs datatrove's SpaCyTokenizer.__init__ sets for these languages.
DATATROVE_CONFIGS = {
    "zh": {"nlp": {"tokenizer": {"segmenter": "jieba"}}},
    "ja": {"nlp": {"tokenizer": {"@tokenizers": "datatrove.ja.JapaneseTokenizer"}}},
}


@lru_cache(maxsize=None)
def load(code: str) -> bool:
    """Registers `code`'s tokenizer data with Rust; False when Rust can't reproduce datatrove's tokenizer here.

    jieba's dictionary and HMM tables are read from its live state at this moment, so words a program adds to
    jieba before the first Chinese doc are honoured, later changes are not.
    """
    try:
        return {"zh": _load_jieba, "ja": _load_sudachi}[code]()
    except (ImportError, OSError, ValueError, AttributeError, KeyError):
        return False


def _load_jieba() -> bool:
    import jieba
    from jieba import finalseg

    if jieba.__version__ != JIEBA_VERSION:
        return False
    jieba.dt.check_initialized()

    def items(table):
        return [(k, list(row.items())) for k, row in table.items()]

    _core.load_jieba(
        list(jieba.dt.FREQ.items()),
        jieba.dt.total,
        list(finalseg.start_P.items()),
        items(finalseg.trans_P),
        items(finalseg.emit_P),
        list(finalseg.Force_Split_Words),
    )
    return True


def _load_sudachi() -> bool:
    import sudachipy

    if sudachipy.__version__ != _core.sudachi_version():
        return False
    dict_path = sudachipy._find_dict_path("core")
    _core.load_sudachi(sudachipy._DEFAULT_SETTINGFILE, sudachipy._DEFAULT_RESOURCEDIR, dict_path)
    return True
