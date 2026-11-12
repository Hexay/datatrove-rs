//! Port of jieba 0.42.1 `jieba.cut(text, cut_all=False, HMM=True)`, the call spaCy's Chinese tokenizer makes with
//! `segmenter="jieba"`, then spaCy's alignment into doc tokens.

mod dag;
mod hmm;
#[cfg(test)]
mod tests;

pub use hmm::Hmm;

use crate::{align, Span};
use rustc_hash::FxHashMap;

pub struct Jieba {
    freq: FxHashMap<Box<str>, u64>,
    log_total: f64,
    hmm: Hmm,
}

/// `re_han_default`: `[一-鿕a-zA-Z0-9+#&\._%\-]`.
fn is_han_default(c: char) -> bool {
    matches!(c, '\u{4E00}'..='\u{9FD5}' | 'a'..='z' | 'A'..='Z' | '0'..='9' | '+' | '#' | '&' | '.' | '_' | '%' | '-')
}

impl Jieba {
    /// jieba's `FREQ` (word -> count, every word prefix present, 0 if not a word) and `total`.
    pub fn new(freq: impl IntoIterator<Item = (String, u64)>, total: u64, hmm: Hmm) -> Result<Self, String> {
        if total == 0 {
            return Err("jieba total is 0".into());
        }
        let freq = freq.into_iter().map(|(w, f)| (w.into_boxed_str(), f)).collect();
        Ok(Self { freq, log_total: (total as f64).ln(), hmm })
    }

    /// `Tokenizer.gen_pfdict` on the contents of a dict.txt.
    pub fn parse_dict(text: &str) -> Result<(Vec<(String, u64)>, u64), String> {
        let mut freq: FxHashMap<String, u64> = FxHashMap::default();
        let mut total = 0u64;
        for (n, line) in text.strip_suffix('\n').unwrap_or(text).split('\n').enumerate() {
            let mut parts = line.trim_matches(|c: char| c.is_ascii_whitespace() || c == '\x0b').split(' ');
            let (Some(word), Some(f)) = (parts.next(), parts.next()) else {
                return Err(format!("invalid dictionary entry at line {}", n + 1));
            };
            let f: u64 = f.parse().map_err(|_| format!("invalid dictionary entry at line {}", n + 1))?;
            freq.insert(word.to_owned(), f);
            total += f;
            for (i, c) in word.char_indices() {
                freq.entry(word[..i + c.len_utf8()].to_owned()).or_insert(0);
            }
        }
        Ok((freq.into_iter().collect(), total))
    }

    fn freq(&self, word: &str) -> Option<u64> {
        self.freq.get(word).copied()
    }

    /// `jieba.cut(text)`: words as byte spans tiling `text`.
    pub fn cut(&self, text: &str, out: &mut Vec<Span>) {
        out.clear();
        let mut i = 0;
        while i < text.len() {
            let c = text[i..].chars().next().unwrap();
            if is_han_default(c) {
                let end = text[i..].find(|c| !is_han_default(c)).map_or(text.len(), |k| i + k);
                self.cut_dag(text, i, end, out);
                i = end;
            } else {
                // `re_skip_default` splits the rest into "\r\n" and single chars.
                let len = if text[i..].starts_with("\r\n") { 2 } else { c.len_utf8() };
                out.push((i as u32, (i + len) as u32));
                i += len;
            }
        }
    }

    /// spaCy's `ChineseTokenizer.__call__` with jieba: doc tokens of `text`, offset by `base`.
    pub fn tokenize(&self, text: &str, base: u32, words: &mut Vec<Span>, out: &mut Vec<Span>) {
        self.cut(text, words);
        let strs: Vec<&str> = words.iter().map(|&(a, b)| &text[a as usize..b as usize]).collect();
        align::align(text, &strs, base, align::text_space, out).expect("jieba words tile the text");
    }
}
