//! `datatrove.pipeline.filters.C4QualityFilter.filter`: drops a doc or rewrites it to its kept lines.

use super::Verdict;
use crate::pystr::{char_len, lower, split_ws, splitlines, strip};
use crate::pyunicode::py;
use crate::words::{SentenceCount, Words};
use std::borrow::Cow;

const END_PUNCTUATION: [&str; 5] = [".", "?", "!", "\"", "'"];
const POLICY_SUBSTRINGS: [&str; 6] =
    ["terms of use", "privacy policy", "cookie policy", "uses cookies", "use of cookies", "use cookies"];

pub struct C4Config {
    pub split_paragraph: bool,
    pub remove_citations: bool,
    pub filter_no_terminal_punct: bool,
    pub min_num_sentences: i64,
    pub min_words_per_line: i64,
    pub max_word_length: i64,
    pub filter_lorem_ipsum: bool,
    pub filter_javascript: bool,
    pub filter_curly_bracket: bool,
    pub filter_policy: bool,
}

/// What happened to each visited line; each maps to datatrove's per-line `stat_update` labels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineOutcome {
    TooLongWord,
    NoTerminalPunc,
    TooFewWords,
    Javascript,
    Policy,
    Kept,
    /// The line dropped the whole doc (lorem ipsum / curly bracket): only `line-total` was counted.
    DroppedDoc,
}

pub struct C4Outcome {
    pub verdict: Verdict,
    /// The rewritten `doc.text` when kept.
    pub text: Option<String>,
    pub lines: Vec<LineOutcome>,
}

pub struct C4 {
    cfg: C4Config,
    words: Words,
}

impl C4 {
    pub fn new(cfg: C4Config, words: Words) -> Self {
        Self { cfg, words }
    }

    /// None when only datatrove can decide: the tokenizer's sentence split isn't ported and the count matters,
    /// or datatrove would raise.
    pub fn filter(&mut self, text: &str) -> Option<C4Outcome> {
        let c = &self.cfg;
        let raw_lines = if c.split_paragraph { splitlines(text) } else { self.words.sentences(text)? };
        let mut lines = Vec::with_capacity(raw_lines.len());
        let mut kept: Vec<String> = Vec::new();
        // A lower bound when some counts are only known to be >= 1 (`SentenceCount::AtLeastOne`).
        let mut num_sentences: i64 = 0;
        let mut bound_only = false;
        let mut line_l = String::new();
        let dropped = |reason: &'static str, lines| {
            Some(C4Outcome { verdict: Verdict::Drop(Cow::Borrowed(reason)), text: None, lines })
        };

        for raw in raw_lines {
            let stripped = strip(raw);
            let n_words = split_ws(stripped).count() as i64;
            if c.max_word_length != -1 && split_ws(stripped).any(|w| char_len(w) as i64 > c.max_word_length) {
                lines.push(LineOutcome::TooLongWord);
                continue;
            }
            let line: Cow<str> = if c.remove_citations { remove_citations(stripped) } else { Cow::Borrowed(stripped) };
            if c.filter_no_terminal_punct
                && (!END_PUNCTUATION.iter().any(|p| line.ends_with(p)) || line.ends_with("..."))
            {
                lines.push(LineOutcome::NoTerminalPunc);
                continue;
            }
            if n_words < c.min_words_per_line {
                lines.push(LineOutcome::TooFewWords);
                continue;
            }
            lower(&line, &mut line_l);
            if c.filter_lorem_ipsum && line_l.contains("lorem ipsum") {
                lines.push(LineOutcome::DroppedDoc);
                return dropped("lorem_ipsum", lines);
            }
            if c.filter_javascript && line_l.contains("javascript") {
                lines.push(LineOutcome::Javascript);
                continue;
            }
            if c.filter_curly_bracket && line.contains('{') {
                lines.push(LineOutcome::DroppedDoc);
                return dropped("curly_bracket", lines);
            }
            if c.filter_policy && POLICY_SUBSTRINGS.iter().any(|p| line_l.contains(p)) {
                lines.push(LineOutcome::Policy);
                continue;
            }
            if c.min_num_sentences != -1 {
                let count = if c.split_paragraph { self.words.sentence_count(&line) } else { SentenceCount::Exact(1) };
                num_sentences += match count {
                    SentenceCount::Exact(n) => n as i64,
                    SentenceCount::AtLeastOne => {
                        bound_only = true;
                        1
                    }
                    SentenceCount::Unknown => return None,
                };
            }
            kept.push(line.into_owned());
            lines.push(LineOutcome::Kept);
        }
        if num_sentences < c.min_num_sentences {
            return if bound_only { None } else { dropped("too_few_sentences", lines) };
        }
        let joined = kept.join(if c.split_paragraph { "\n" } else { " " });
        Some(C4Outcome { verdict: Verdict::Keep, text: Some(strip(&joined).to_owned()), lines })
    }
}

/// `CITATION_REGEX.sub("", line)` for `\[\d*]|\[edit]|\[citation needed]`.
fn remove_citations(line: &str) -> Cow<'_, str> {
    if !line.contains('[') {
        return Cow::Borrowed(line);
    }
    let d = &py().re_d;
    let mut out = String::with_capacity(line.len());
    let mut i = 0;
    while let Some(off) = line[i..].find('[') {
        let at = i + off;
        out.push_str(&line[i..at]);
        let rest = &line[at + 1..];
        let digits = rest.find(|ch: char| !d.contains(ch)).unwrap_or(rest.len());
        let len = if rest[digits..].starts_with(']') {
            digits + 2
        } else if rest.starts_with("edit]") {
            6
        } else if rest.starts_with("citation needed]") {
            17
        } else {
            out.push('[');
            i = at + 1;
            continue;
        };
        i = at + len;
    }
    out.push_str(&line[i..]);
    Cow::Owned(out)
}
