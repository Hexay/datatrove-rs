//! `IndicNLPTokenizer.word_tokenize`: indic-nlp-library's `trivial_tokenize` (punctuation splitting, then the
//! `pat_num_seq` number re-joining with its quirk of skipping a match at position 0).

use super::indic_sents::{sentence_count, sentences};
use super::{SentenceCount, Splitter, Src};
use crate::pystr::strip;
use rustc_hash::FxHashMap;
use serde::Deserialize;
use spacy_tokenizer_rs::rules::CharSet;
use std::sync::OnceLock;

const JSON: &str = include_str!("../../data/wordtok/indic.json");

#[derive(Deserialize)]
struct Data {
    indic_punct: Vec<(u32, u32)>,
    urdu_punct: Vec<(u32, u32)>,
    delim_danda: Vec<(u32, u32)>,
    delim_no_danda: Vec<(u32, u32)>,
    contains_danda: Vec<(u32, u32)>,
    isnumeric: Vec<(u32, u32)>,
    danda_langs: Vec<String>,
    script_base: FxHashMap<String, u32>,
    coordinated_end: u32,
    sinhala_to_devanagari: Vec<(u32, u32)>,
    ack_chars: Vec<String>,
}

pub(super) struct Tables {
    indic_punct: CharSet,
    urdu_punct: CharSet,
    pub delim_danda: CharSet,
    pub delim_no_danda: CharSet,
    pub contains_danda: CharSet,
    pub isnumeric: CharSet,
    pub danda_langs: Vec<String>,
    pub script_base: FxHashMap<String, u32>,
    pub coordinated_end: u32,
    pub sinhala_to_devanagari: FxHashMap<char, char>,
    pub ack_chars: rustc_hash::FxHashSet<String>,
}

pub(super) fn tables() -> &'static Tables {
    static T: OnceLock<Tables> = OnceLock::new();
    T.get_or_init(|| {
        let d: Data = serde_json::from_str(JSON).expect("embedded indic.json");
        let ch = |cp: u32| char::from_u32(cp).expect("valid code point");
        Tables {
            indic_punct: CharSet::new(&d.indic_punct),
            urdu_punct: CharSet::new(&d.urdu_punct),
            delim_danda: CharSet::new(&d.delim_danda),
            delim_no_danda: CharSet::new(&d.delim_no_danda),
            contains_danda: CharSet::new(&d.contains_danda),
            isnumeric: CharSet::new(&d.isnumeric),
            danda_langs: d.danda_langs,
            script_base: d.script_base,
            coordinated_end: d.coordinated_end,
            sinhala_to_devanagari: d.sinhala_to_devanagari.into_iter().map(|(a, b)| (ch(a), ch(b))).collect(),
            ack_chars: d.ack_chars.into_iter().collect(),
        }
    })
}

pub(super) struct Indic {
    pub lang: &'static str,
}

impl Indic {
    pub fn new(lang: &'static str) -> Self {
        Self { lang }
    }

    fn urdu(&self) -> bool {
        self.lang == "ur"
    }
}

impl Splitter for Indic {
    fn split(&self, text: &str, arena: &mut String, out: &mut Vec<(u32, u32)>) -> Src {
        let t = tables();
        let punct = if self.urdu() { &t.urdu_punct } else { &t.indic_punct };
        let mut s = String::with_capacity(text.len() + 16);
        spaced(text, punct, &mut s);
        arena.clear();
        if self.urdu() {
            std::mem::swap(arena, &mut s);
        } else {
            join_numbers(&s, arena);
        }
        out.clear();
        let mut at = 0;
        for piece in arena.split(' ') {
            let w = strip(piece);
            if !w.is_empty() {
                let a = at + (w.as_ptr() as usize - piece.as_ptr() as usize);
                out.push((a as u32, (a + w.len()) as u32));
            }
            at += piece.len() + 1;
        }
        Src::Arena
    }

    fn sentence_count(&self, text: &str) -> SentenceCount {
        if self.urdu() {
            // indic-nlp-library hands Urdu to urduhack
            return SentenceCount::Unknown;
        }
        sentence_count(text, self.lang)
    }

    fn has_sentences(&self) -> bool {
        !self.urdu()
    }

    fn sentences(&self, text: &str) -> Option<Vec<String>> {
        self.has_sentences().then(|| sentences(text, self.lang))
    }
}

/// `re.sub('[ ]+', ' ', punct_pat.sub(r' \1 ', text.replace('\t', ' '))).strip(' ')`.
fn spaced(text: &str, punct: &CharSet, s: &mut String) {
    let mut pending = false;
    for c in text.chars() {
        if c == ' ' || c == '\t' {
            pending = true;
            continue;
        }
        let p = punct.contains(c);
        if (pending || p) && !s.is_empty() {
            s.push(' ');
        }
        s.push(c);
        pending = p;
    }
}

/// trivial_tokenize_indic's loop over `pat_num_seq` = `([0-9]+ [,.:/] )+[0-9]+` matches: spaces are removed
/// inside each match, except one starting at 0 (`if start > prev`, with `prev` starting at 0).
fn join_numbers(s: &str, out: &mut String) {
    let b = s.as_bytes();
    let mut prev = 0;
    let mut i = 0;
    while i < b.len() {
        if !b[i].is_ascii_digit() {
            i += 1;
            continue;
        }
        match num_seq_end(b, i) {
            Some(end) => {
                if i > prev {
                    out.push_str(&s[prev..i]);
                    out.extend(s[i..end].chars().filter(|&c| c != ' '));
                    prev = end;
                }
                i = end;
            }
            None => i = digits_end(b, i),
        }
    }
    out.push_str(&s[prev..]);
}

fn digits_end(b: &[u8], mut i: usize) -> usize {
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    i
}

/// End of the greedy `pat_num_seq` match starting at `i` (a digit), if any.
fn num_seq_end(b: &[u8], mut i: usize) -> Option<usize> {
    let mut groups = 0;
    let mut end = None;
    loop {
        let d = digits_end(b, i);
        if d == i {
            return end;
        }
        if groups > 0 {
            end = Some(d);
        }
        if b.len() > d + 2 && b[d] == b' ' && matches!(b[d + 1], b',' | b'.' | b':' | b'/') && b[d + 2] == b' ' {
            groups += 1;
            i = d + 3;
        } else {
            return end;
        }
    }
}
