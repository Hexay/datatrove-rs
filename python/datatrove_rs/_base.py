import importlib.util
import unicodedata
import warnings
from functools import lru_cache, partial

from datatrove.utils.word_tokenizers import SpaCyTokenizer, WordTokenizer, load_tokenizer_assignments

from datatrove_rs import _core, _wordtok, cjk

_NOT_APPLICABLE = False


@lru_cache(maxsize=1)
def spacy_installed() -> bool:
    return importlib.util.find_spec("spacy") is not None


@lru_cache(maxsize=1)
def rust_languages() -> frozenset[str]:
    """spaCy codes whose tokenizer rules are embedded in Rust (each one checked against spaCy, see docs/parity.md)."""
    return frozenset(_core.spacy_languages())


@lru_cache(maxsize=None)
def rust_language(language) -> str | None:
    """The Rust tokenizer for `language`: the spaCy code when datatrove would use a plain `spacy.blank(code)` that
    Rust covers (or its zh/ja config when `cjk.load` succeeds), else a `_wordtok` port's name.

    Decided from datatrove's factory table without instantiating the tokenizer, which would require spaCy (or the
    other tokenizer's library) to be installed even though the Rust path never uses it.
    """
    if isinstance(language, WordTokenizer):
        if type(language) is not SpaCyTokenizer:
            return _wordtok.rust_name_of(language)
        if language.config != cjk.DATATROVE_CONFIGS.get(language.language):
            return None
        cls_name, code = "SpaCyTokenizer", language.language
    else:
        factory = load_tokenizer_assignments().get(language)
        cls_name, code = factory.args if isinstance(factory, partial) and not factory.keywords else (None, None)
    if cls_name == "SpaCyTokenizer" and code in cjk.DATATROVE_CONFIGS:
        return code if cjk.load(code) else None
    if cls_name == "SpaCyTokenizer" and code in rust_languages():
        return code
    return _wordtok.rust_name(cls_name, code)


def check_unicode_tables():
    py_version, ucd = _core.unicode_tables_version()
    if ucd != unicodedata.unidata_version:
        warnings.warn(
            f"datatrove_rs was built with Unicode {ucd} tables (Python {py_version}) but this Python uses "
            f"Unicode {unicodedata.unidata_version}; results may differ from datatrove on rare characters.",
            stacklevel=3,
        )


class DropsRustOnPickle:
    """The Rust object in `_rust` is not picklable, and datatrove pickles steps to ship them to workers, so it is
    dropped on pickling and rebuilt lazily."""

    _rust = None

    def __getstate__(self):
        state = self.__dict__.copy()
        state.pop("_rust", None)
        return state


class RustBacked(DropsRustOnPickle):
    """Mixin: builds the Rust implementation lazily and falls back to datatrove's Python code for other languages."""

    def _build_rust(self, language: str):
        """The Rust implementation for spaCy code `language`."""
        raise NotImplementedError

    def rust(self):
        """The Rust implementation, or None when this step must run datatrove's Python code."""
        if self._rust is None:
            code = rust_language(self.language)
            self._rust = self._build_rust(code) if code else _NOT_APPLICABLE
        return self._rust or None

    def rust_for(self, text: str):
        """`rust()`, or None when `text` must go through datatrove's Python code.

        spaCy 3.8 raises inside `memory_zone` on a token "IS_ALPHA" once any earlier call in the process has seen
        one, and datatrove then retokenizes with "IS_ALPHA" deleted. That depends on the shared spaCy vocab's
        history, so such docs run through datatrove itself to keep that state and the output exact. Without spaCy
        installed there is no such state to match (datatrove itself could not run), so Rust handles them too.
        """
        return None if "IS_ALPHA" in text and spacy_installed() else self.rust()
