from datatrove.pipeline import filters as dt

from datatrove_rs import _core
from datatrove_rs._base import RustBacked
from datatrove_rs._run import Counters, FastRun

# Indexed by the line outcome codes C4Quality.filter returns; code 6 = the line dropped the whole doc.
_C4_LINE_STATS = (
    "line-filter-too_long_word",
    "line-filter-no_terminal_punc",
    "line-filter-too_few_words",
    "line-filter-javascript",
    "line-filter-policy",
    "line-kept",
)


class _RustFilter(RustBacked, FastRun):
    """`filter()` via Rust when `rust_for` allows it, else datatrove's; `run()` per `FastRun`."""

    def filter(self, doc):
        rs = self.rust_for(doc.text)
        return self._rust_filter(rs, doc) if rs else super().filter(doc)

    def _rust_filter(self, rs, doc, counters=None):
        return rs.filter(doc.text)

    def _doc_filter(self, rs, counters):
        rust_filter, py_filter = self._rust_filter, self.filter

        def filter_doc(doc):
            return py_filter(doc) if "IS_ALPHA" in doc.text else rust_filter(rs, doc, counters)

        return filter_doc


class GopherQualityFilter(_RustFilter, dt.GopherQualityFilter):
    def _build_rust(self):
        return _core.GopherQuality(
            self.min_doc_words,
            self.max_doc_words,
            self.min_avg_word_length,
            self.max_avg_word_length,
            self.max_symbol_word_ratio,
            self.max_bullet_lines_ratio,
            self.max_ellipsis_lines_ratio,
            self.max_non_alpha_words_ratio,
            self.min_stop_words,
            list(self.stop_words),
        )


class GopherRepetitionFilter(_RustFilter, dt.GopherRepetitionFilter):
    def _build_rust(self):
        return _core.GopherRepetition(
            self.dup_line_frac,
            self.dup_para_frac,
            self.dup_line_char_frac,
            self.dup_para_char_frac,
            list(self.top_n_grams),
            list(self.dup_n_grams),
        )


class FineWebQualityFilter(_RustFilter, dt.FineWebQualityFilter):
    def _build_rust(self):
        return _core.FineWebQuality(
            self.line_punct_thr,
            self.line_punct_exclude_zero,
            list(self.stop_chars),
            self.short_line_threshold,
            self.short_line_length,
            self.char_duplicates_ratio,
            self.new_line_ratio,
        )


class C4QualityFilter(_RustFilter, dt.C4QualityFilter):
    def _build_rust(self):
        return _core.C4Quality(
            self.split_paragraph,
            self.remove_citations,
            self.filter_no_terminal_punct,
            self.min_num_sentences,
            self.min_words_per_line,
            self.max_word_length,
            self.filter_lorem_ipsum,
            self.filter_javascript,
            self.filter_curly_bracket,
            self.filter_policy,
        )

    def _rust_filter(self, rs, doc, counters=None):
        result, text, lines = rs.filter(doc.text)
        _count_lines(counters or Counters(self.stats.stats), lines)
        if text is not None:
            doc.text = text
        return result


def _count_lines(counters, codes: bytes):
    """C4's per-line `stat_update`s. Once every label involved exists, per-label totals give the same stats."""
    if not codes:
        return
    stats = counters.stats
    counts = [(label, codes.count(code)) for code, label in enumerate(_C4_LINE_STATS)]
    if "line-total" in stats and all(label in stats for label, k in counts if k):
        counters.add("line-total", len(codes))
        for label, k in counts:
            if k:
                counters.add(label, k)
        return
    for code in codes:
        counters.add("line-total")
        if code < len(_C4_LINE_STATS):
            counters.add(_C4_LINE_STATS[code])
