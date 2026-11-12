import re

from datatrove.pipeline.filters import c4_filters as dt

from datatrove_rs import _core
from datatrove_rs._base import DropsRustOnPickle

_PREFIX, _SUFFIX = r"(?:\W|^)(", r")(?:\W|$)"


def _words(regex: re.Pattern) -> tuple[list[str], bool] | None:
    """The word list and form of a datatrove badwords regex, or None unless rebuilding it gives the same pattern."""
    pattern = regex.pattern
    bounded = pattern.startswith(_PREFIX) and pattern.endswith(_SUFFIX)
    body = pattern[len(_PREFIX) : -len(_SUFFIX)] if bounded else pattern
    words, cur, chars = [], [], iter(body)
    for c in chars:
        if c == "\\":
            cur.append(next(chars, ""))
        elif c == "|":
            words.append("".join(cur))
            cur = []
        else:
            cur.append(c)
    words.append("".join(cur))
    rebuilt = "|".join(re.escape(w) for w in words)
    if bounded:
        rebuilt = _PREFIX + rebuilt + _SUFFIX
    return (words, bounded) if rebuilt == pattern and regex.flags == re.UNICODE else None


class _RustSearch:
    """Stands in for the compiled regex: `filter()` only tests `search(text) is not None`."""

    __slots__ = ("regex", "rs")

    def __init__(self, regex: re.Pattern, rs):
        self.regex, self.rs = regex, rs

    def search(self, text: str):
        try:
            return self.rs.search(text) or None
        except UnicodeEncodeError:
            return self.regex.search(text)


class C4BadWordsFilter(DropsRustOnPickle, dt.C4BadWordsFilter):
    """Word lists, `keep_fraction` sampling and stats stay datatrove's; the search runs in Rust
    (`crates/datatrove-core/src/filters/badwords.rs`)."""

    def _get_badwords(self, lang: str):
        regex = super()._get_badwords(lang)
        if not isinstance(regex, re.Pattern):
            return regex
        if self._rust is None:
            self._rust = {}
        if lang not in self._rust:
            parsed = _words(regex)
            self._rust[lang] = _RustSearch(regex, _core.BadWords(*parsed)) if parsed else regex
        return self._rust[lang]
