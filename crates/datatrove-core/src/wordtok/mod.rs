//! Ports of datatrove's non-spaCy word tokenizers (`datatrove.utils.word_tokenizers`), keyed by a Rust name:
//! `indic:<code>` for `IndicNLPTokenizer(code)`, `thai` for `ThaiTokenizer`, `vi` for `SpaCyTokenizer("vi")`
//! (pyvi). Data from `tools/export_wordtok.py` and `tools/export_vi.py`.

mod indic;
mod indic_sents;
mod thai;
mod vi;

use crate::pystr::char_len;
use std::sync::OnceLock;

/// Where `Splitter::split`'s spans point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Src {
    Text,
    Arena,
}

/// `len(split_into_sentences(text))` as far as Rust can tell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SentenceCount {
    Exact(usize),
    /// Not computed, but datatrove's splitter returns at least one piece and never raises here.
    AtLeastOne,
    /// Only datatrove can tell (it may raise).
    Unknown,
}

pub trait Splitter: Sync + Send {
    /// `word_tokenize(text)` after datatrove's `strip_strings`: byte spans into `text` or into `arena`.
    /// For a `spacy_doc` splitter: the spaCy `Doc` token spans in `text`, before stripping.
    fn split(&self, text: &str, arena: &mut String, out: &mut Vec<(u32, u32)>) -> Src;

    /// Sentence count for splitters whose sentences don't come from a spaCy `Doc`.
    fn sentence_count(&self, _text: &str) -> SentenceCount {
        SentenceCount::Unknown
    }

    /// True when `sentences` is ported.
    fn has_sentences(&self) -> bool {
        false
    }

    /// `sent_tokenize(text)` (after `strip_strings`) for splitters whose sentences don't come from a spaCy `Doc`;
    /// None unless `has_sentences`.
    fn sentences(&self, _text: &str) -> Option<Vec<String>> {
        None
    }

    /// True when `split` gives a spaCy `Doc`'s tokens, which the `sentencizer` pipe then splits like any spaCy
    /// language's.
    fn spacy_doc(&self) -> bool {
        false
    }
}

const INDIC: [&str; 16] =
    ["as", "bn", "gu", "hi", "kK", "kn", "ml", "mr", "ne", "or", "pa", "sa", "si", "ta", "te", "ur"];

/// Every Rust tokenizer name.
pub fn names() -> impl Iterator<Item = String> {
    INDIC.iter().map(|l| format!("indic:{l}")).chain(["thai".to_owned(), "vi".to_owned()])
}

/// datatrove's `simple_span_tokenize(text, sents)` end offsets in chars; None where it raises (a sentence not
/// found after the previous one).
pub fn simple_span_ends(text: &str, sents: &[String]) -> Option<Vec<usize>> {
    if sents.len() == 1 {
        return Some(vec![char_len(text)]);
    }
    let mut ends = Vec::with_capacity(sents.len());
    let (mut byte, mut chars) = (0, 0);
    for s in sents {
        let off = text[byte..].find(s.as_str())?;
        chars += char_len(&text[byte..byte + off]) + char_len(s);
        byte += off + s.len();
        ends.push(chars);
    }
    Some(ends)
}

/// The splitter called `name`, built on first use.
pub fn splitter(name: &str) -> Option<&'static dyn Splitter> {
    static INDICS: OnceLock<Vec<indic::Indic>> = OnceLock::new();
    static THAI: OnceLock<thai::Thai> = OnceLock::new();
    static VI: OnceLock<vi::Vietnamese> = OnceLock::new();
    match name {
        "thai" => return Some(THAI.get_or_init(thai::Thai::new)),
        "vi" => return Some(VI.get_or_init(vi::Vietnamese::new)),
        _ => {}
    }
    let lang = name.strip_prefix("indic:")?;
    let i = INDIC.iter().position(|&l| l == lang)?;
    Some(&INDICS.get_or_init(|| INDIC.iter().map(|l| indic::Indic::new(l)).collect())[i])
}
