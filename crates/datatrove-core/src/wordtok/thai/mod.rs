//! `ThaiTokenizer`: pythainlp `word_tokenize(text, keep_whitespace=False, engine="newmm-safe")` with the
//! default dictionary, `rejoin_formatted_num`, then datatrove's `strip_strings`.

mod newmm;
mod tcc;
mod trie;

use super::{SentenceCount, Splitter, Src};
use crate::pystr::strip;
use crate::pyunicode::py;
use trie::Trie;

const WORDS: &str = include_str!("../../../data/wordtok/thai_words.txt");

pub(super) struct Thai {
    dict: Trie,
}

impl Thai {
    pub fn new() -> Self {
        Self { dict: Trie::new(WORDS.split('\n')) }
    }
}

impl Splitter for Thai {
    fn split(&self, text: &str, _arena: &mut String, out: &mut Vec<(u32, u32)>) -> Src {
        out.clear();
        let chars: Vec<char> = text.chars().collect();
        let mut byte = Vec::with_capacity(chars.len() + 1);
        byte.extend(text.char_indices().map(|(i, _)| i));
        byte.push(text.len());
        let mut ends = Vec::new();
        newmm::segment(&chars, &self.dict, &mut ends);
        let mut prev = 0;
        for (a, b) in rejoin_formatted_num(&chars, &ends) {
            debug_assert_eq!(a, prev);
            prev = b;
            let tok = &text[byte[a]..byte[b]];
            let w = strip(tok);
            if !w.is_empty() {
                let s = byte[a] + (w.as_ptr() as usize - tok.as_ptr() as usize);
                out.push((s as u32, (s + w.len()) as u32));
            }
        }
        Src::Text
    }

    /// datatrove's Thai sentences come from pythainlp's crfcut model, which isn't ported; its pieces are
    /// contiguous text, so `simple_span_tokenize` finds them and the count is at least one.
    fn sentence_count(&self, _text: &str) -> SentenceCount {
        SentenceCount::AtLeastOne
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
