//! `SentenceDedupFilter.remove_dup_sentences` and the word/sentence counts its keep decision uses.

use crate::pystr::{lstrip, splitlines};
use crate::words::{SentenceCount, Words};

pub struct DupRemover {
    n: i64,
    split_sentences: bool,
    min_words_to_remove_span: f64,
    words: Words,
}

/// `remove_dup_sentences`' result plus what its `stat_update`s need.
pub struct Removed {
    pub text: String,
    pub original: String,
    /// `len(sentence_spans)`.
    pub sentences: usize,
    /// `len(kept_sentences)`.
    pub kept: usize,
}

impl DupRemover {
    pub fn new(n: i64, split_sentences: bool, min_words_to_remove_span: f64, words: Words) -> Self {
        Self { n, split_sentences, min_words_to_remove_span, words }
    }

    /// False when `split_sentences` needs sentence spans Rust doesn't have for this language.
    pub fn supported(&self) -> bool {
        !self.split_sentences || self.words.has_sentences()
    }

    /// `len(word_tokenize(text))`.
    pub fn word_count(&mut self, text: &str) -> usize {
        let mut words = Vec::new();
        self.words.words(text, &mut words);
        words.len()
    }

    /// `len(split_into_sentences(text))` as far as Rust can tell.
    pub fn sentence_count(&mut self, text: &str) -> SentenceCount {
        self.words.sentence_count(text)
    }

    /// `remove_dup_sentences(text, dups)`. None where datatrove's code must run: no Rust sentence spans for this
    /// text, or `dups` out of order, where datatrove raises.
    pub fn remove(&mut self, text: &str, dups: &[u16]) -> Option<Removed> {
        let pieces = if self.split_sentences {
            let ends = self.words.sentence_ends(text)?;
            let chars = CharIndex::new(text);
            let mut last = 0;
            ends.iter()
                .map(|&e| {
                    let s = chars.slice(text, last, e);
                    last = e;
                    s
                })
                .collect()
        } else {
            splitlines(text)
        };
        let mut kept: Vec<&str> = Vec::with_capacity(pieces.len());
        let mut original: Vec<&str> = Vec::with_capacity(pieces.len() + 4);
        // Only the first sentence of a dropped run is kept here (datatrove appends only while it is empty).
        let mut removed: Option<&str> = None;
        let (mut dup_i, mut drop_until) = (0, 0i64);
        for (idx, &line) in pieces.iter().enumerate() {
            let idx = idx as i64;
            if let Some(&d) = dups.get(dup_i) {
                if (d as i64) < idx {
                    return None;
                }
                if d as i64 == idx {
                    drop_until = idx + self.n;
                    dup_i += 1;
                }
            }
            if idx >= drop_until {
                if let Some(r) = removed.take() {
                    original.push("<<<");
                    if self.restores(r) {
                        kept.push(r);
                    }
                }
                kept.push(line);
            } else if removed.is_none() {
                removed = Some(line);
                original.push(">>>");
            }
            original.push(line);
        }
        if let Some(r) = removed {
            original.push("<<<");
            if self.restores(r) {
                kept.push(r);
            }
        }
        let merge = if self.split_sentences { "" } else { "\n" };
        Some(Removed {
            text: lstrip(&kept.join(merge)).to_owned(),
            original: original.join(merge),
            sentences: pieces.len(),
            kept: kept.len(),
        })
    }

    /// Whether a dropped run whose span list is `[first]` goes back into the text (`min_words_to_remove_span`).
    fn restores(&mut self, first: &str) -> bool {
        self.min_words_to_remove_span > 0.0 && (self.word_count(first) as f64) < self.min_words_to_remove_span
    }
}

/// Python `str` slicing by code point offsets.
enum CharIndex {
    Ascii(usize),
    /// Byte offset of each char, plus the text length.
    Bytes(Vec<usize>),
}

impl CharIndex {
    fn new(text: &str) -> Self {
        if text.is_ascii() {
            return CharIndex::Ascii(text.len());
        }
        CharIndex::Bytes(text.char_indices().map(|(i, _)| i).chain([text.len()]).collect())
    }

    /// `text[a:b]` for non-negative `a`, `b`.
    fn slice<'a>(&self, text: &'a str, a: usize, b: usize) -> &'a str {
        let byte = |k: usize| match self {
            CharIndex::Ascii(n) => k.min(*n),
            CharIndex::Bytes(v) => v[k.min(v.len() - 1)],
        };
        if a >= b {
            return "";
        }
        &text[byte(a)..byte(b)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn remover(n: i64, split: bool, min_words: f64) -> DupRemover {
        DupRemover::new(n, split, min_words, Words::default())
    }

    #[test]
    fn lines_mode_marks_removed_runs() {
        let r = remover(2, false, 0.0).remove("a\nb\nc\nd", &[1]).unwrap();
        assert_eq!(r.text, "a\nd");
        assert_eq!(r.original, "a\n>>>\nb\nc\n<<<\nd");
        assert_eq!((r.sentences, r.kept), (4, 2));
    }

    #[test]
    fn out_of_order_dups_fall_back() {
        assert!(remover(1, false, 0.0).remove("a\nb\nc", &[1, 1]).is_none());
    }

    #[test]
    fn short_run_restores_first_sentence_only() {
        let r = remover(2, false, 3.0).remove("a\nb b\nc\nd", &[1]).unwrap();
        assert_eq!(r.text, "a\nb b\nd");
        assert_eq!(r.kept, 3);
    }

    #[test]
    fn sentence_mode_slices_between_ends() {
        let r = remover(1, true, 0.0).remove("Héllo there. Bye now.", &[0]).unwrap();
        assert_eq!(r.text, "Bye now.");
        assert_eq!(r.original, ">>>Héllo there.<<< Bye now.");
    }
}
