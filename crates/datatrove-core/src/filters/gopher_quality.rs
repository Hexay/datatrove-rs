//! `datatrove.pipeline.filters.GopherQualityFilter.filter`.

use super::{div, drop, on, FilterResult, Verdict};
use crate::pystr::{char_len, lstrip, rstrip, splitlines};
use crate::pyunicode::py;
use crate::words::Words;
use rustc_hash::FxHashSet;

pub struct GopherQualityConfig {
    pub min_doc_words: Option<f64>,
    pub max_doc_words: Option<f64>,
    pub min_avg_word_length: Option<f64>,
    pub max_avg_word_length: Option<f64>,
    pub max_symbol_word_ratio: Option<f64>,
    pub max_bullet_lines_ratio: Option<f64>,
    pub max_ellipsis_lines_ratio: Option<f64>,
    pub max_non_alpha_words_ratio: Option<f64>,
    pub min_stop_words: Option<f64>,
    pub stop_words: Vec<String>,
}

pub struct GopherQuality {
    cfg: GopherQualityConfig,
    stop_words: FxHashSet<String>,
    words: Words,
}

impl GopherQuality {
    pub fn new(cfg: GopherQualityConfig, words: Words) -> Self {
        let stop_words = cfg.stop_words.iter().cloned().collect();
        Self { cfg, stop_words, words }
    }

    pub fn filter(&mut self, text: &str) -> FilterResult {
        let c = &self.cfg;
        let t = py();
        let mut words = Vec::new();
        self.words.words(text, &mut words);
        let n_words = words.len();

        let non_symbol: Vec<usize> = words
            .iter()
            .filter(|w| w.chars().any(|ch| !t.dt_punctuation_set.contains(ch)))
            .map(|w| char_len(w))
            .collect();
        let n_non_symbol = non_symbol.len() as f64;

        if on(c.min_doc_words).is_some_and(|m| n_non_symbol < m) {
            return drop("gopher_short_doc");
        }
        if on(c.max_doc_words).is_some_and(|m| n_non_symbol > m) {
            return drop("gopher_long_doc");
        }

        // np.mean of an empty list is nan, and every comparison with nan is False.
        let avg = if non_symbol.is_empty() { f64::NAN } else { non_symbol.iter().sum::<usize>() as f64 / n_non_symbol };
        if on(c.min_avg_word_length).is_some_and(|m| avg < m) {
            return drop("gopher_below_avg_threshold");
        }
        if on(c.max_avg_word_length).is_some_and(|m| avg > m) {
            return drop("gopher_above_avg_threshold");
        }

        if let Some(m) = on(c.max_symbol_word_ratio) {
            if div(text.matches('#').count(), n_words)? > m {
                return drop("gopher_too_many_hashes");
            }
        }
        if let Some(m) = on(c.max_symbol_word_ratio) {
            if div(text.matches("...").count() + text.matches('…').count(), n_words)? > m {
                return drop("gopher_too_many_ellipsis");
            }
        }

        let lines = splitlines(text);
        if let Some(m) = on(c.max_bullet_lines_ratio) {
            let bullets = lines.iter().filter(|s| lstrip(s).starts_with(['•', '-'])).count();
            if div(bullets, lines.len())? > m {
                return drop("gopher_too_many_bullets");
            }
        }
        if let Some(m) = on(c.max_ellipsis_lines_ratio) {
            let ends = lines.iter().filter(|s| rstrip(s).ends_with("...") || rstrip(s).ends_with('…')).count();
            if div(ends, lines.len())? > m {
                return drop("gopher_too_many_end_ellipsis");
            }
        }

        if let Some(m) = on(c.max_non_alpha_words_ratio) {
            let alpha = words.iter().filter(|w| w.chars().any(|ch| t.isalpha.contains(ch))).count();
            if div(alpha, n_words)? < m {
                return drop("gopher_below_alpha_threshold");
            }
        }

        if let Some(m) = on(c.min_stop_words) {
            let found: FxHashSet<&str> = words.iter().copied().filter(|w| self.stop_words.contains(*w)).collect();
            if (found.len() as f64) < m {
                return drop("gopher_enough_stop_words");
            }
        }

        Ok(Verdict::Keep)
    }
}
