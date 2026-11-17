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
# pythainlp's crfcut sentence splitter tags with python-crfsuite; the Thai port reproduces these releases' tagger.
VERIFIED_CRFSUITE = ("python-crfsuite", frozenset({"0.9.12"}))
# indic-nlp-library's "ur" sentence_split is urduhack's sentence_tokenizer; see docs/parity.md.
VERIFIED_URDUHACK = ("urduhack", frozenset({"1.0.2", "1.0.3"}))
_CLASSES = {"ThaiTokenizer": ThaiTokenizer, "IndicNLPTokenizer": IndicNLPTokenizer}


def _version(dist: str) -> str | None:
    try:
        return importlib.metadata.version(dist)
    except importlib.metadata.PackageNotFoundError:
        return None


@lru_cache(maxsize=None)
def _library_ok(cls_name: str) -> bool:
    dist, versions = VERIFIED[cls_name]
    version = _version(dist)
    return version is None or version in versions


@lru_cache(maxsize=1)
def load_thai_sentences() -> bool:
    """Loads pythainlp's crfcut model and word sets into the Thai port; True when pythainlp and python-crfsuite are
    installed at verified versions (otherwise Thai sentences stay datatrove's)."""
    dist, versions = VERIFIED["ThaiTokenizer"]
    crf_dist, crf_versions = VERIFIED_CRFSUITE
    if _version(dist) not in versions or _version(crf_dist) not in crf_versions:
        return False
    from pythainlp.corpus import corpus_path
    from pythainlp.tokenize import crfcut
    from pythainlp.tools.path import safe_path_join

    model = safe_path_join(corpus_path(), crfcut._CRFCUT_DATA_FILENAME)
    _core.load_thai_crfcut(model, sorted(crfcut._ENDERS), sorted(crfcut._STARTERS))
    return True


@lru_cache(maxsize=1)
def load_urdu_sentences() -> bool:
    """Gives the Urdu port urduhack's word lists; True when urduhack is at a verified version and imports (it needs
    TensorFlow), otherwise Urdu sentences stay datatrove's. Only sentence steps call it, to spare the import."""
    dist, versions = VERIFIED_URDUHACK
    if _version(dist) not in versions:
        return False
    try:
        from urduhack.tokenization import eos, sentence_tokenizer  # noqa: F401  (datatrove's import)
    except Exception:
        return False
    _core.load_urdu_eos(list(eos._URDU_CONJUNCTIONS), list(eos._URDU_NEWLINE_WORDS))
    return True


def load_sentences(name: str | None) -> None:
    """Loads the library data a port's sentence splitter reads at run time, before a sentence step builds."""
    if name == "indic:ur":
        load_urdu_sentences()


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
    if name == "thai":
        load_thai_sentences()
    return name


def rust_name_of(tok) -> str | None:
    """`rust_name` for a tokenizer instance of exactly one of the ported classes."""
    cls_name = next((n for n, c in _CLASSES.items() if type(tok) is c), None)
    return rust_name(cls_name, tok.language) if cls_name else None


@lru_cache(maxsize=None)
def c4_supported(name: str, split_paragraph: bool) -> bool:
    """Rust C4 with `split_paragraph=False` needs whole-text sentence spans: "vi" (a spaCy Doc), and Thai and Urdu
    with their splitters loaded. Without it, Thai sentence counts are left to datatrove (only needed when a doc keeps
    few lines); Rust skips them when every kept line counts at least one, which requires datatrove's Thai splitter
    (pythainlp's crfcut) to be importable."""
    if name == "vi" or (name == "thai" and load_thai_sentences()) or (name == "indic:ur" and load_urdu_sentences()):
        return True
    if not split_paragraph:
        return False
    return name != "thai" or all(importlib.util.find_spec(m) for m in ("pythainlp", "pycrfsuite"))
