//! `spacy.blank("vi")` with pyvi: spaCy's `pyvi_sylabelize_with_ws` regex, pyvi's CRF word segmentation, the
//! merge rules of spaCy's `pyvi_tokenize`, then `get_words_and_spaces`, which adds whitespace tokens for gaps.

mod crf;

use super::{Splitter, Src};
use crate::pyunicode::py;
use crf::{Crf, I_W};
use regex::Regex;

const JSON: &str = include_str!("../../../data/wordtok/vi.json");
/// Python's `string.punctuation`; pyvi tests `token in string.punctuation` (a substring test).
const PUNCTUATION: &str = "!\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~";

pub(super) struct Vietnamese {
    crf: Crf,
    syllable: Regex,
}

/// A character class of Python code point ranges, optionally negated.
fn class(ranges: &[&[(u32, u32)]], negate: bool) -> String {
    let mut s = String::from(if negate { "[^" } else { "[" });
    for &(lo, hi) in ranges.iter().flat_map(|r| r.iter()) {
        s.push_str(&format!("\\x{{{lo:X}}}-\\x{{{hi:X}}}"));
    }
    s.push(']');
    s
}

impl Vietnamese {
    pub fn new() -> Self {
        let [re_s, re_w, re_d] = &py().re_ranges;
        let (s, w, d) = (class(&[re_s], false), class(&[re_w], false), class(&[re_d], false));
        let (not_s, not_ws) = (class(&[re_s], true), class(&[re_w, re_s], true));
        let alts = [
            format!("{s}+"),
            r"[A-ZĐ]+\.|Tp\.|Mr\.|Mrs\.|Ms\.|Dr\.|ThS\.".to_owned(),
            r"==>|->|\.\.\.|>>".to_owned(),
            format!("{w}+://{not_s}+"),
            r"[a-zA-Z0-9_.+-]+@(?:[a-zA-Z0-9-]+\.)+[a-zA-Z0-9-]+".to_owned(),
            format!(r"{d}+(?:[.,_]{d}+)+"),
            not_ws,
            format!("{w}+"),
        ];
        let syllable = Regex::new(&alts.join("|")).expect("syllable regex");
        Self { crf: Crf::new(serde_json::from_str(JSON).expect("embedded vi.json")), syllable }
    }

    /// The text spans of `pyvi_tokenize(text)`.
    fn pyvi_tokens(&self, text: &str, out: &mut Vec<(usize, usize)>) {
        let t = py();
        if text.chars().all(|c| t.isspace.contains(c)) {
            return; // "" -> no words; all-space -> [text], which get_words_and_spaces treats as a gap
        }
        let words: Vec<(usize, usize)> = self
            .syllable
            .find_iter(text)
            .map(|m| (m.start(), m.end()))
            .filter(|&(a, _)| !text[a..].starts_with(|c| t.isspace.contains(c)))
            .collect();
        let strs: Vec<&str> = words.iter().map(|&(a, b)| &text[a..b]).collect();
        let labels = self.crf.tag(&strs);
        let first_digit = |s: &str| s.chars().next().is_some_and(|c| self.crf.digit.contains(c));
        let first_title = |s: &str| s.chars().next().is_some_and(|c| self.crf.is_title(c.encode_utf8(&mut [0; 4])));
        let mut tok = words[0];
        for i in 1..words.len() {
            let (w, p) = (strs[i], strs[i - 1]);
            if labels[i] == I_W
                && !PUNCTUATION.contains(w)
                && !PUNCTUATION.contains(p)
                && !first_digit(w)
                && !first_digit(p)
                && !(first_title(w) && !first_title(p))
            {
                tok.1 = words[i].1;
            } else {
                out.push(tok);
                tok = words[i];
            }
        }
        out.push(tok);
    }
}

impl Splitter for Vietnamese {
    fn split(&self, text: &str, _arena: &mut String, out: &mut Vec<(u32, u32)>) -> Src {
        out.clear();
        let mut toks = Vec::new();
        self.pyvi_tokens(text, &mut toks);
        let mut pos = 0;
        for (a, b) in toks {
            if a > pos {
                out.push((pos as u32, a as u32));
            }
            out.push((a as u32, b as u32));
            pos = b + usize::from(text[b..].starts_with(' '));
        }
        if pos < text.len() {
            out.push((pos as u32, text.len() as u32));
        }
        Src::Text
    }

    fn spacy_doc(&self) -> bool {
        true
    }
}
