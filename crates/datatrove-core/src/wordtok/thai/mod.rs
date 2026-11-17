//! `ThaiTokenizer`: pythainlp `word_tokenize(text, keep_whitespace=False, engine="newmm-safe")` with the
//! default dictionary, `rejoin_formatted_num`, then datatrove's `strip_strings`; sentences are pythainlp's
//! `crfcut` once `load_crfcut` has read its model.

mod crfcut;
mod newmm;
mod tcc;
mod trie;

use super::{sentence_count_of, SentenceCount, Splitter, Src};
use crate::pystr::strip;
use crate::pyunicode::py;
use crfcut::CrfCut;
use std::sync::OnceLock;
use trie::Trie;

const WORDS: &str = include_str!("../../../data/wordtok/thai_words.txt");

static CRFCUT: OnceLock<CrfCut> = OnceLock::new();

/// Loads pythainlp's `sentenceseg_crfcut.model` and crfcut's `_ENDERS`/`_STARTERS`; the first load per process wins.
pub fn load_crfcut(model_path: &str, enders: Vec<String>, starters: Vec<String>) -> Result<(), String> {
    if CRFCUT.get().is_none() {
        let bytes = std::fs::read(model_path).map_err(|e| format!("{model_path}: {e}"))?;
        let _ = CRFCUT.set(CrfCut::new(&bytes, enders, starters)?);
    }
    Ok(())
}

pub(super) struct Thai {
    dict: Trie,
}

impl Thai {
    pub fn new() -> Self {
        Self { dict: Trie::new(WORDS.split('\n')) }
    }

    /// Byte spans of pythainlp's `word_tokenize(text, engine=newmm or newmm-safe)`, whitespace kept.
    fn tokens(&self, text: &str, safe: bool) -> Vec<(usize, usize)> {
        let chars: Vec<char> = text.chars().collect();
        let mut byte = Vec::with_capacity(chars.len() + 1);
        byte.extend(text.char_indices().map(|(i, _)| i));
        byte.push(text.len());
        let mut ends = Vec::new();
        newmm::segment(&chars, &self.dict, safe, &mut ends);
        rejoin_formatted_num(&chars, &ends).into_iter().map(|(a, b)| (byte[a], byte[b])).collect()
    }
}

impl Splitter for Thai {
    fn split(&self, text: &str, _arena: &mut String, out: &mut Vec<(u32, u32)>) -> Src {
        out.clear();
        for (a, b) in self.tokens(text, true) {
            let tok = &text[a..b];
            let w = strip(tok);
            if !w.is_empty() {
                let s = a + (w.as_ptr() as usize - tok.as_ptr() as usize);
                out.push((s as u32, (s + w.len()) as u32));
            }
        }
        Src::Text
    }

    /// Without the crfcut model: its pieces are contiguous text, so `simple_span_tokenize` finds them and the
    /// count is at least one.
    fn sentence_count(&self, text: &str) -> SentenceCount {
        match self.sentences(text) {
            Some(sents) => sentence_count_of(text, &sents),
            None => SentenceCount::AtLeastOne,
        }
    }

    fn has_sentences(&self) -> bool {
        CRFCUT.get().is_some()
    }

    /// `sent_tokenize(text)`: `crfcut.segment`, then `strip_strings`.
    fn sentences(&self, text: &str) -> Option<Vec<String>> {
        let crf = CRFCUT.get()?;
        let pieces = crf.segment(text, &self.tokens(text, false));
        Some(pieces.into_iter().map(strip).filter(|s| !s.is_empty()).map(str::to_owned).collect())
    }
}

/// pythainlp `_utils.rejoin_formatted_num` over contiguous segments given by their char ends; returns the
/// resulting (start, end) char spans.
fn rejoin_formatted_num(text: &[char], ends: &[usize]) -> Vec<(usize, usize)> {
    let mut matches = formatted_nums(text).into_iter().peekable();
    let mut out = Vec::with_capacity(ends.len());
    let (mut pos, mut i) = (0, 0);
    while i < ends.len() {
        let Some(&(m_start, m_end)) = matches.peek() else { break };
        if pos >= m_start {
            let start = pos;
            while pos < m_end && i < ends.len() {
                pos = ends[i];
                i += 1;
            }
            if pos > start {
                out.push((start, pos));
            }
            matches.next();
        } else {
            out.push((pos, ends[i]));
            pos = ends[i];
            i += 1;
        }
    }
    for &e in &ends[i..] {
        out.push((pos, e));
        pos = e;
    }
    out
}

/// Matches of `(\d+[\.\,:])+\d+` (char spans); a match is a maximal digits-separator chain, so each one starts at
/// a digit run.
fn formatted_nums(text: &[char]) -> Vec<(usize, usize)> {
    let d = |c: &char| py().re_d.contains(*c);
    let run = |from: usize| from + text[from..].iter().take_while(|c| d(c)).count();
    let mut out = Vec::new();
    let mut i = 0;
    while i < text.len() {
        if !d(&text[i]) {
            i += 1;
            continue;
        }
        let mut end = run(i);
        let mut groups = 0;
        while matches!(text.get(end), Some('.' | ',' | ':')) && text.get(end + 1).is_some_and(d) {
            end = run(end + 1);
            groups += 1;
        }
        if groups > 0 {
            out.push((i, end));
        }
        i = end;
    }
    out
}
