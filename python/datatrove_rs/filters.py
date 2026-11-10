from datatrove.pipeline import filters as dt

from datatrove_rs import _core
from datatrove_rs._base import RustBacked

# Indexed by the line outcome codes C4Quality.filter returns; None = the line dropped the whole doc.
_C4_LINE_STATS = (
    "line-filter-too_long_word",
    "line-filter-no_terminal_punc",
    "line-filter-too_few_words",
    "line-filter-javascript",
    "line-filter-policy",
    "line-kept",
    None,
)


class GopherQualityFilter(RustBacked, dt.GopherQualityFilter):
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

    def filter(self, doc):
        rs = self.rust_for(doc.text)
        return rs.filter(doc.text) if rs else super().filter(doc)


class GopherRepetitionFilter(RustBacked, dt.GopherRepetitionFilter):
    def _build_rust(self):
        return _core.GopherRepetition(
            self.dup_line_frac,
            self.dup_para_frac,
            self.dup_line_char_frac,
            self.dup_para_char_frac,
            list(self.top_n_grams),
            list(self.dup_n_grams),
        )

    def filter(self, doc):
        rs = self.rust_for(doc.text)
        return rs.filter(doc.text) if rs else super().filter(doc)


class FineWebQualityFilter(RustBacked, dt.FineWebQualityFilter):
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

    def filter(self, doc):
        rs = self.rust_for(doc.text)
        return rs.filter(doc.text) if rs else super().filter(doc)


class C4QualityFilter(RustBacked, dt.C4QualityFilter):
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

    def filter(self, doc):
        rs = self.rust_for(doc.text)
        if not rs:
            return super().filter(doc)
        result, text, lines = rs.filter(doc.text)
        for code in lines:
            label = _C4_LINE_STATS[code]
            if label:
                self.stat_update("line-total", label)
            else:
                self.stat_update("line-total")
        if text is not None:
            doc.text = text
        return result
