//! `datatrove.pipeline.filters.GopherRepetitionFilter.filter`.

mod dup;
mod joined;
#[cfg(test)]
mod tests;
mod top;

use super::{div, drop, on, FilterResult, Verdict};
use crate::pystr::{char_len, strip};
use crate::words::Words;
use rustc_hash::FxHashSet;
use std::borrow::Cow;

pub struct GopherRepetitionConfig {
    pub dup_line_frac: Option<f64>,
    pub dup_para_frac: Option<f64>,
    pub dup_line_char_frac: Option<f64>,
    pub dup_para_char_frac: Option<f64>,
    pub top_n_grams: Vec<(usize, f64)>,
    pub dup_n_grams: Vec<(usize, f64)>,
}

pub struct GopherRepetition {
    cfg: GopherRepetitionConfig,
    words: Words,
    joined: joined::Joined,
    top: top::TopNGrams,
    dup: dup::DupNGrams,
}

/// `find_duplicates`: (duplicate element count, their total char length).
pub fn find_duplicates<'a>(items: impl IntoIterator<Item = &'a str>) -> (usize, usize) {
    let mut seen = FxHashSet::default();
    let (mut elements, mut chars) = (0, 0);
    for x in items {
        if !seen.insert(x) {
            elements += 1;
            chars += char_len(x);
        }
    }
    (elements, chars)
}

/// `re.split(r"\n{min,}", s)` (min = 1 is `\n+`): pieces between maximal newline runs of at least `min`.
fn split_newline_runs(s: &str, min: usize) -> Vec<&str> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let (mut start, mut i) = (0, 0);
    while let Some(off) = s[i..].find('\n') {
        let a = i + off;
        let mut e = a;
        while e < b.len() && b[e] == b'\n' {
            e += 1;
        }
        if e - a >= min {
            out.push(&s[start..a]);
            start = e;
        }
        i = e;
    }
    out.push(&s[start..]);
    out
}

impl GopherRepetition {
    pub fn new(cfg: GopherRepetitionConfig) -> Self {
        Self { cfg, words: Words::default(), joined: Default::default(), top: Default::default(), dup: Default::default() }
    }

    pub fn filter(&mut self, text: &str) -> FilterResult {
        if text.is_empty() {
            return drop("empty");
        }
        let c = &self.cfg;
        let text_len = char_len(text);

        let paragraphs = split_newline_runs(strip(text), 2);
        let (para_dups, char_dups) = find_duplicates(paragraphs.iter().copied());
        if on(c.dup_para_frac).is_some_and(|m| para_dups as f64 / paragraphs.len() as f64 > m) {
            return drop("dup_para_frac");
        }
        if let Some(m) = on(c.dup_para_char_frac) {
            if div(char_dups, text_len)? > m {
                return drop("dup_para_char_frac");
            }
        }

        let lines = split_newline_runs(text, 1);
        let (line_dups, char_dups) = find_duplicates(lines.iter().copied());
        if on(c.dup_line_frac).is_some_and(|m| line_dups as f64 / lines.len() as f64 > m) {
            return drop("dup_line_frac");
        }
        if let Some(m) = on(c.dup_line_char_frac) {
            if div(char_dups, text_len)? > m {
                return drop("dup_line_char_frac");
            }
        }

        let mut words = Vec::new();
        self.words.words(text, &mut words);
        if c.top_n_grams.is_empty() && c.dup_n_grams.is_empty() {
            return Ok(Verdict::Keep);
        }
        self.joined.build(&words);
        if !c.top_n_grams.is_empty() {
            self.top.prepare(&self.joined);
            for &(n, frac) in &c.top_n_grams {
                // top <= bound and float division is monotone, so a bound that passes proves the n-gram passes.
                if let Some(bound) = self.top.bound(&self.joined, n) {
                    if !(div(bound, text_len)? > frac) {
                        continue;
                    }
                }
                let Some(top) = self.top.top_chars(&self.joined, n) else { continue };
                if div(top, text_len)? > frac {
                    return Ok(Verdict::Drop(Cow::Owned(format!("top_{n}_gram"))));
                }
            }
        }

        if c.dup_n_grams.is_empty() {
            return Ok(Verdict::Keep);
        }
        self.dup.prepare(&self.joined);
        for &(n, frac) in &c.dup_n_grams {
            if div(self.dup.duplicate_chars(&self.joined, n), text_len)? > frac {
                return Ok(Verdict::Drop(Cow::Owned(format!("duplicated_{n}_n_grams"))));
            }
        }

        Ok(Verdict::Keep)
    }
}
