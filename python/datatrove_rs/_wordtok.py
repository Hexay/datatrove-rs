"""Which of datatrove's non-spaCy word tokenizers run in Rust (`crates/datatrove-core/src/wordtok`).

The ports embed data from one library version (`tools/export_wordtok.py`), so they run only when that library is
absent (datatrove could not tokenize at all then) or installed at a version in `VERIFIED`; see docs/parity.md.
"""

import importlib.metadata
import importlib.util
from functools import lru_cache

from datatrove.utils.word_tokenizers import IndicNLPTokenizer, ThaiTokenizer

from datatrove_rs import _core

# datatrove tokenizer class -> (distribution, versions the Rust port matches on FineWeb-2 and fuzzed text).
# `SpaCyTokenizer` here means only "vi", the one spaCy language whose tokenizer is a library (pyvi).
VERIFIED = {
    "ThaiTokenizer": ("pythainlp", frozenset({"5.4.0"})),
    "IndicNLPTokenizer": ("indic-nlp-library", frozenset({"0.92"})),
    "SpaCyTokenizer": ("pyvi", frozenset({"0.1.1"})),
}
_CLASSES = {"ThaiTokenizer": ThaiTokenizer, "IndicNLPTokenizer": IndicNLPTokenizer}


@lru_cache(maxsize=None)
def _library_ok(cls_name: str) -> bool:
    dist, versions = VERIFIED[cls_name]
    try:
        return importlib.metadata.version(dist) in versions
    except importlib.metadata.PackageNotFoundError:
        return True


def _name(cls_name: str, arg: str | None) -> str | None:
    if cls_name == "ThaiTokenizer":
        return "thai"
    if cls_name == "IndicNLPTokenizer":
        return f"indic:{arg}"
    return "vi" if cls_name == "SpaCyTokenizer" and arg == "vi" else None


@lru_cache(maxsize=1)
def names() -> frozenset[str]:
    return frozenset(_core.word_tokenizers())


def rust_name(cls_name: str | None, arg: str | None) -> str | None:
    """The Rust tokenizer for datatrove's `<cls_name>(arg)`, or None."""
    name = _name(cls_name, arg) if cls_name in VERIFIED else None
    if name is None or name not in names() or not _library_ok(cls_name):
        return None
    return name


def rust_name_of(tok) -> str | None:
    """`rust_name` for a tokenizer instance of exactly one of the ported classes."""
    cls_name = next((n for n, c in _CLASSES.items() if type(tok) is c), None)
    return rust_name(cls_name, tok.language) if cls_name else None


@lru_cache(maxsize=None)
def c4_supported(name: str, split_paragraph: bool) -> bool:
    """Rust C4 with `split_paragraph=False` needs whole-text sentence spans, which only "vi" (a spaCy Doc) has.
    Thai sentence counts are left to datatrove (only needed when a doc keeps few lines); Rust skips them when every
    kept line counts at least one, which requires datatrove's Thai splitter (pythainlp's crfcut) to be importable."""
    if name == "vi":
        return True
    if not split_paragraph:
        return False
    return name != "thai" or all(importlib.util.find_spec(m) for m in ("pythainlp", "pycrfsuite"))
