//! `extract_stats` of datatrove's WordStats, SentenceStats, DocStats, LineStats and ParagraphStats: the values
//! in each step's dict order. Ratios are `_safe_divide` of two ints (0.0 when the denominator is 0).

use super::Num;
use crate::filters::gopher_repetition::find_duplicates;
use crate::pystr::{char_len, is_title, is_upper, splitlines, strip};
use crate::pyunicode::py;
use crate::words::Words;
use rustc_hash::FxHashSet;

fn ratio(a: usize, b: usize) -> Num {
    Num::Float(if b == 0 { 0.0 } else { a as f64 / b as f64 })
}

fn count(n: impl Iterator<Item = bool>) -> usize {
    n.filter(|&b| b).count()
}

/// The `short_*` (len <= t) then `long_*` (len >= t) ratios every step has.
fn push_length_ratios(out: &mut Vec<Num>, lens: &[usize], short: &[i64], long: &[i64]) {
    let len = |l: usize| l as i64;
    out.extend(short.iter().map(|&t| ratio(count(lens.iter().map(|&l| len(l) <= t)), lens.len())));
    out.extend(long.iter().map(|&t| ratio(count(lens.iter().map(|&l| len(l) >= t)), lens.len())));
}

pub struct WordStats {
    pub words: Words,
    pub short: Vec<i64>,
    pub long: Vec<i64>,
    pub stop_words: FxHashSet<String>,
}

impl WordStats {
    fn extract(&mut self, text: &str, out: &mut Vec<Num>) {
        let mut words = Vec::new();
        self.words.words(text, &mut words);
        let n = words.len();
        let lens: Vec<usize> = words.iter().map(|w| char_len(w)).collect();
        out.push(Num::Int(n as i64));
        out.push(ratio(lens.iter().sum(), n));
        out.push(ratio(n, splitlines(text).len()));
        push_length_ratios(out, &lens, &self.short, &self.long);
        out.push(ratio(words.iter().collect::<FxHashSet<_>>().len(), n));
        out.push(ratio(count(words.iter().map(|w| is_upper(w))), n));
        out.push(ratio(count(words.iter().map(|w| is_title(w))), n));
        out.push(ratio(count(words.iter().map(|w| self.stop_words.contains(*w))), n));
    }
}

pub struct SentenceStats {
    pub words: Words,
    pub short: Vec<i64>,
    pub long: Vec<i64>,
}

impl SentenceStats {
    fn extract(&mut self, text: &str, out: &mut Vec<Num>) -> bool {
        let Some(lens) = self.words.sentence_lengths(text) else { return false };
        out.push(Num::Int(lens.len() as i64));
        out.push(ratio(lens.iter().sum(), lens.len()));
        push_length_ratios(out, &lens, &self.short, &self.long);
        true
    }
}

pub struct DocStats;

impl DocStats {
    fn extract(text: &str, out: &mut Vec<Num>) {
        let t = py();
        let (mut n, mut space, mut other, mut digit, mut upper, mut punct, mut ellipsis) = (0, 0, 0, 0, 0, 0, 0);
        for c in text.chars() {
            n += 1;
            let is_digit = t.isdigit.contains(c);
            space += t.isspace.contains(c) as usize;
            other += (!t.isalpha.contains(c) && !is_digit) as usize;
            digit += is_digit as usize;
            upper += t.isupper.contains(c) as usize;
            punct += t.dt_punctuation.contains(c) as usize;
            ellipsis += (c == '…') as usize;
        }
        // `(?:\.\.\.)|(?:…)` findall: the alternatives share no char, so "..." counts non-overlapping
        ellipsis += 3 * text.matches("...").count();
        out.push(Num::Int(n as i64));
        out.extend([space, other, digit, upper, ellipsis, punct].map(|k| ratio(k, n)));
    }
}

pub struct LineStats {
    pub short: Vec<i64>,
    pub long: Vec<i64>,
    pub ignore_empty_lines: bool,
}

impl LineStats {
    /// For `text.split("\n")`; the caller handles `doc.metadata["lines"]`.
    fn extract(&self, text: &str, out: &mut Vec<Num>) {
        let mut lines: Vec<&str> = text.split('\n').collect();
        out.push(Num::Int(lines.len() as i64));
        if self.ignore_empty_lines {
            lines.retain(|l| !strip(l).is_empty());
        }
        let lens: Vec<usize> = lines.iter().map(|l| char_len(l)).collect();
        let total: usize = lens.iter().sum();
        out.push(ratio(total, lines.len()));
        push_length_ratios(out, &lens, &self.short, &self.long);
        let terminal = |l: &str| l.ends_with(['.', '?', '!', '"', '\'']);
        let bullet = |l: &str| strip(l).starts_with(['-', '*', '•']);
        out.push(ratio(count(lines.iter().map(|l| terminal(l))), lines.len()));
        out.push(ratio(count(lines.iter().map(|l| bullet(l))), lines.len()));
        let (dups, dup_chars) = find_duplicates(lines.iter().copied());
        out.push(ratio(dups, lines.len()));
        out.push(ratio(dup_chars, total));
    }
}

pub struct ParagraphStats {
    pub short: Vec<i64>,
    pub long: Vec<i64>,
}

impl ParagraphStats {
    fn extract(&self, text: &str, out: &mut Vec<Num>) {
        let paragraphs: Vec<&str> = text.split("\n\n").filter(|p| !strip(p).is_empty()).collect();
        let lens: Vec<usize> = paragraphs.iter().map(|p| char_len(p)).collect();
        let total: usize = lens.iter().sum();
        out.push(Num::Int(paragraphs.len() as i64));
        out.push(ratio(total, paragraphs.len()));
        push_length_ratios(out, &lens, &self.short, &self.long);
        let (dups, dup_chars) = find_duplicates(paragraphs.iter().copied());
        out.push(ratio(dups, paragraphs.len()));
        out.push(ratio(dup_chars, total));
    }
}

pub enum Extractor {
    Word(WordStats),
    Sentence(SentenceStats),
    Doc(DocStats),
    Line(LineStats),
    Paragraph(ParagraphStats),
}

impl Extractor {
    /// The step's `extract_stats(doc)` values into `out`; false when Rust has no sentence splitter for the
    /// language (Thai, Urdu), so datatrove's code must run.
    pub fn extract(&mut self, text: &str, out: &mut Vec<Num>) -> bool {
        out.clear();
        match self {
            Extractor::Word(s) => s.extract(text, out),
            Extractor::Sentence(s) => return s.extract(text, out),
            Extractor::Doc(_) => DocStats::extract(text, out),
            Extractor::Line(s) => s.extract(text, out),
            Extractor::Paragraph(s) => s.extract(text, out),
        }
        true
    }
}
