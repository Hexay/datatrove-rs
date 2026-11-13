"""datatrove's WordStats, SentenceStats, DocStats, LineStats and ParagraphStats with `extract_stats` and
`BaseStats.run`'s bookkeeping in Rust (`_stats_run.py`). See docs/parity.md."""

from datatrove.pipeline import stats as dt

from datatrove_rs import _core
from datatrove_rs._base import _NOT_APPLICABLE, RustBacked
from datatrove_rs._stats_run import FastStatsRun, datatrove_verified


def _ints(values) -> list[int] | None:
    """Thresholds without duplicates (their dict keys collapse), or None unless all are plain ints."""
    values = list(values)
    return list(dict.fromkeys(values)) if all(type(v) is int for v in values) else None


class _Extract:
    """A Rust extractor and the step's stat names; `__call__` is `extract_stats(doc)` or None."""

    __slots__ = ("rs", "names")

    def __init__(self, rs, names):
        self.rs, self.names = rs, names

    def __call__(self, text):
        values = self.rs.extract(text)
        return None if values is None else dict(zip(self.names, values))


class _RustStats(FastStatsRun, RustBacked):
    """`extract_stats` via Rust when it applies to the doc, else datatrove's; `run()` per `FastStatsRun`."""

    _tokenized = True

    def _build(self, language):
        """The `_Extract` for this step (`language`: the Rust tokenizer name), or `_NOT_APPLICABLE`."""
        raise NotImplementedError

    def _build_rust(self, language):
        return self._build(language) if datatrove_verified() else _NOT_APPLICABLE

    def _python_only(self, doc) -> bool:
        return False

    def extract_stats(self, doc):
        rs = self.rust_for(doc.text) if self._tokenized else self.rust()
        if rs and not self._python_only(doc):
            try:
                out = rs(doc.text)
            except (_core.TokenizerFallback, UnicodeEncodeError):
                out = None
            if out is not None:
                return out
        return super().extract_stats(doc)


class _UntokenizedStats(_RustStats):
    _tokenized = False

    def rust(self):
        if self._rust is None:
            self._rust = self._build_rust(None) or _NOT_APPLICABLE
        return self._rust or None


def _ratio_names(prefix, short, long):
    return [f"short_{prefix}_{c}" for c in short] + [f"long_{prefix}_{c}" for c in long]


class WordStats(_RustStats, dt.WordStats):
    def _build(self, language):
        short, long = _ints(self.short_word_max_chars_threshold), _ints(self.long_word_max_chars_threshold)
        stop_words = self.stop_words
        if short is None or long is None or type(stop_words) not in (list, tuple, set, frozenset):
            return _NOT_APPLICABLE
        if not all(type(w) is str for w in stop_words):
            return _NOT_APPLICABLE
        names = ["n_words", "avg_word_length", "avg_words_per_line", *_ratio_names("word_ratio", short, long)]
        names += ["type_token_ratio", "uppercase_word_ratio", "capitalized_word_ratio", "stop_word_ratio"]
        return _Extract(_core.StatsExtractor.words(language, short, long, list(stop_words)), names)


class SentenceStats(_RustStats, dt.SentenceStats):
    def _build(self, language):
        short, long = _ints(self.short_sentence_max_chars_threshold), _ints(self.long_sentence_max_chars_threshold)
        if short is None or long is None:
            return _NOT_APPLICABLE
        names = ["n_sentences", "avg_sentence_length", *_ratio_names("sentence_ratio", short, long)]
        return _Extract(_core.StatsExtractor.sentences(language, short, long), names)


class DocStats(_UntokenizedStats, dt.DocStats):
    _NAMES = ("length", "white_space_ratio", "non_alpha_digit_ratio", "digit_ratio", "uppercase_ratio")

    def _build(self, language):
        return _Extract(_core.StatsExtractor.doc(), [*self._NAMES, "elipsis_ratio", "punctuation_ratio"])


class LineStats(_UntokenizedStats, dt.LineStats):
    def _build(self, language):
        short, long = _ints(self.short_max_chars), _ints(self.long_max_chars)
        if short is None or long is None:
            return _NOT_APPLICABLE
        names = ["n_lines", "avg_line_length", *_ratio_names("line_ratio_chars", short, long)]
        names += ["lines_ending_with_terminal_mark_ratio", "bullet_point_lines_ratio"]
        names += ["line_duplicates", "line_char_duplicates"]
        return _Extract(_core.StatsExtractor.lines(short, long, bool(self.ignore_empty_lines)), names)

    def _python_only(self, doc) -> bool:
        return bool(doc.metadata.get("lines"))


class ParagraphStats(_UntokenizedStats, dt.ParagraphStats):
    def _build(self, language):
        short = _ints(self.short_paragraph_max_chars_threshold)
        long = _ints(self.long_paragraph_max_chars_threshold)
        if short is None or long is None:
            return _NOT_APPLICABLE
        names = ["n_paragraphs", "avg_paragraph_length", *_ratio_names("paragraph_ratio", short, long)]
        names += ["paragraph_duplicates", "paragraph_char_duplicates"]
        return _Extract(_core.StatsExtractor.paragraphs(short, long), names)
