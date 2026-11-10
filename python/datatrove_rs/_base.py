import unicodedata
import warnings
from functools import lru_cache

from datatrove.utils.word_tokenizers import SpaCyTokenizer, load_word_tokenizer

from datatrove_rs import _core

_NOT_APPLICABLE = False


@lru_cache(maxsize=None)
def uses_spacy_english(language) -> bool:
    """True when datatrove would tokenize `language` with plain `spacy.blank("en")`, the only tokenizer ported."""
    try:
        tok = load_word_tokenizer(language)
    except ValueError:
        return False
    return type(tok) is SpaCyTokenizer and tok.language == "en" and tok.config is None


def check_unicode_tables():
    py_version, ucd = _core.unicode_tables_version()
    if ucd != unicodedata.unidata_version:
        warnings.warn(
            f"datatrove_rs was built with Unicode {ucd} tables (Python {py_version}) but this Python uses "
            f"Unicode {unicodedata.unidata_version}; results may differ from datatrove on rare characters.",
            stacklevel=3,
        )


class RustBacked:
    """Mixin: builds the Rust implementation lazily and falls back to datatrove's Python code for other languages.

    The Rust object is not picklable, and datatrove pickles steps to ship them to workers, so it is dropped on pickling.
    """

    _rust = None

    def _build_rust(self):
        raise NotImplementedError

    def rust(self):
        """The Rust implementation, or None when this step must run datatrove's Python code."""
        if self._rust is None:
            self._rust = self._build_rust() if uses_spacy_english(self.language) else _NOT_APPLICABLE
        return self._rust or None

    def rust_for(self, text: str):
        """`rust()`, or None when `text` must go through datatrove's Python code.

        spaCy 3.8 raises inside `memory_zone` on a token "IS_ALPHA" once any earlier call in the process has seen
        one, and datatrove then retokenizes with "IS_ALPHA" deleted. That depends on the shared spaCy vocab's
        history, so such docs run through datatrove itself to keep that state and the output exact.
        """
        return None if "IS_ALPHA" in text else self.rust()

    def __getstate__(self):
        state = self.__dict__.copy()
        state.pop("_rust", None)
        return state
