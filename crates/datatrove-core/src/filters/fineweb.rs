//! `datatrove.pipeline.filters.FineWebQualityFilter.filter`.

use super::gopher_repetition::find_duplicates;
use super::{div, drop, FilterResult, Verdict};
use crate::pystr::{char_len, strip};
use crate::words::Words;

pub struct FineWebConfig {
    pub line_punct_thr: f64,
    pub line_punct_exclude_zero: bool,
    pub stop_chars: Vec<String>,
    pub short_line_thr: f64,
    pub short_line_length: i64,
    pub char_duplicates_ratio: f64,
    pub new_line_ratio: f64,
}

pub struct FineWeb {
    cfg: FineWebConfig,
    words: Words,
}

impl FineWeb {
    pub fn new(cfg: FineWebConfig, words: Words) -> Self {
        Self { cfg, words }
    }

    pub fn filter(&mut self, text: &str) -> FilterResult {
        let c = &self.cfg;
        let lines: Vec<&str> = text.split('\n').filter(|l| !strip(l).is_empty()).collect();
        if lines.is_empty() {
            return drop("empty");
        }
        let n = lines.len();

        let punct = lines.iter().filter(|l| c.stop_chars.iter().any(|s| l.ends_with(s.as_str()))).count();
        let ratio = punct as f64 / n as f64;
        if ratio < c.line_punct_thr && !(ratio == 0.0 && c.line_punct_exclude_zero) {
            return drop("line_punct_ratio");
        }

        let short = lines.iter().filter(|l| char_len(l) as i64 <= c.short_line_length).count();
        if short as f64 / n as f64 > c.short_line_thr {
            return drop("short_line_ratio");
        }

        let non_newline = char_len(text) - text.matches('\n').count();
        if div(find_duplicates(lines.iter().copied()).1, non_newline)? > c.char_duplicates_ratio {
            return drop("char_dup_ratio");
        }

        let mut words = Vec::new();
        self.words.words(text, &mut words);
        if div(text.matches('\n').count(), words.len())? > c.new_line_ratio {
            return drop("list_ratio");
        }

        Ok(Verdict::Keep)
    }
}
