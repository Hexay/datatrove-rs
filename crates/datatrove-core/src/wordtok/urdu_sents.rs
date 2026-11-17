//! urduhack 1.0.2-1.1.1 `tokenization.sentence_tokenizer` (`eos._generate_sentences`), which indic-nlp-library's
//! `sentence_split` returns for "ur". The word lists come from the installed urduhack (`load_urdu_eos`).

use crate::pystr::split_ws;
use rustc_hash::FxHashSet;
use std::sync::OnceLock;

struct Eos {
    conjunctions: FxHashSet<String>,
    newline_words: FxHashSet<String>,
}

static EOS: OnceLock<Eos> = OnceLock::new();

/// Takes `eos._URDU_CONJUNCTIONS` and `eos._URDU_NEWLINE_WORDS`; the first load per process wins.
pub fn load_urdu_eos(conjunctions: Vec<String>, newline_words: Vec<String>) {
    let set = |v: Vec<String>| v.into_iter().collect();
    let _ = EOS.set(Eos { conjunctions: set(conjunctions), newline_words: set(newline_words) });
}

pub(super) fn loaded() -> bool {
    EOS.get().is_some()
}

/// `_generate_sentences(text)`, unchanged by datatrove's `strip_strings` (each sentence is two or more
/// whitespace-free words joined by one space). None when not loaded or where urduhack raises.
pub(super) fn sentences(text: &str) -> Option<Vec<String>> {
    let eos = EOS.get()?;
    // `_split_and_keep` splits on chr(ord(max(text)) + 1), a ValueError past U+10FFFF
    if text.contains('\u{10FFFF}') {
        return None;
    }
    let mut out = Vec::new();
    for piece in text.split_inclusive('۔') {
        if split_ws(piece).nth(1).is_none() {
            continue;
        }
        if piece.contains('؟') {
            for q in piece.split_inclusive('؟') {
                eos.push_sentences(q, &mut out);
            }
        } else {
            eos.push_sentences(piece, &mut out);
        }
    }
    Some(out)
}

impl Eos {
    /// The per-piece loop: a line break after a verb-final word not followed by a conjunction (after the
    /// following "۔"/"،" when there is one, which is then skipped), keeping groups of two or more words.
    fn push_sentences(&self, piece: &str, out: &mut Vec<String>) {
        let words: Vec<&str> = split_ws(piece).collect();
        let mut group: Vec<&str> = Vec::new();
        let mut flush = |group: &mut Vec<&str>| {
            if group.len() >= 2 {
                out.push(group.join(" "));
            }
            group.clear();
        };
        let mut i = 0;
        while i < words.len() {
            let word = words[i];
            group.push(word);
            let next = words.get(i + 1).copied();
            i += 1;
            if let Some(next) = next.filter(|n| self.newline_words.contains(word) && !self.conjunctions.contains(*n)) {
                if next == "۔" || next == "،" {
                    group.push(next);
                    i += 1;
                }
                flush(&mut group);
            }
        }
        flush(&mut group);
    }
}
