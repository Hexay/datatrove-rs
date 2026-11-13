//! datatrove's sentence deduplication: `SentenceDedupSignature.get_hashes` and the sorted signature records
//! `save_hashes` writes (stage 1), and `SentenceDedupFilter.remove_dup_sentences` (stage 3, `remove`).

mod remove;

pub use remove::{DupRemover, Removed};

use crate::minhash::HashConfig;
use crate::pystr::{splitlines, strip};
use crate::simplify::{NormConfig, Simplifier};
use crate::words::Words;
use std::borrow::Cow;

/// (hash, doc index, n-gram index), datatrove's signature tuple.
pub type Sig = (u64, u64, u64);

pub struct Signer {
    n: usize,
    split_sentences: bool,
    hash: HashConfig,
    simplifier: Simplifier,
    words: Words,
    simple: String,
    ends: Vec<usize>,
    joined: String,
}

impl Signer {
    pub fn new(n: usize, split_sentences: bool, norm: NormConfig, hash: HashConfig, words: Words) -> Self {
        let (simple, ends, joined) = Default::default();
        Self { n, split_sentences, hash, simplifier: Simplifier::new(norm), words, simple, ends, joined }
    }

    /// False when `split_sentences` needs a sentence splitter Rust doesn't have for this language.
    pub fn supported(&self) -> bool {
        !self.split_sentences || self.words.has_sentences()
    }

    /// `get_hashes(doc, _)` as (hash, n-gram index) pairs: one per n-sentence window whose `" ".join` of simplified
    /// sentences isn't blank. False (and `out` empty) where `supported` is false.
    pub fn hashes(&mut self, text: &str, out: &mut Vec<(u64, u64)>) -> bool {
        out.clear();
        let sents: Vec<Cow<str>> = if self.split_sentences {
            match self.words.sentence_texts(text) {
                Some(s) => s,
                None => return false,
            }
        } else {
            splitlines(text).into_iter().map(Cow::Borrowed).collect()
        };
        if self.n == 0 || sents.len() < self.n {
            return true;
        }
        self.simple.clear();
        self.ends.clear();
        for s in &sents {
            let simple = self.simplifier.simplify(s);
            self.simple.push_str(simple);
            self.ends.push(self.simple.len());
        }
        for i in 0..=sents.len() - self.n {
            self.joined.clear();
            for k in i..i + self.n {
                if k > i {
                    self.joined.push(' ');
                }
                let start = if k == 0 { 0 } else { self.ends[k - 1] };
                self.joined.push_str(&self.simple[start..self.ends[k]]);
            }
            if !strip(&self.joined).is_empty() {
                out.push((self.hash.hash(&self.joined), i as u64));
            }
        }
        true
    }
}

/// `save_hashes`' array after `np.array(sigs, dtype=[hash, ("doc", "<u4"), ("sent", "<u2")])` and `sort`: packed
/// little-endian records sorted by (hash, doc, sent). None when a value doesn't fit its field, where numpy raises
/// or wraps, so the caller must hand the tuples to datatrove.
pub fn sorted_records(sigs: &mut [Sig], precision: u32) -> Option<Vec<u8>> {
    let hash_max = if precision == 32 { u32::MAX as u64 } else { u64::MAX };
    if sigs.iter().any(|&(h, d, s)| h > hash_max || d > u32::MAX as u64 || s > u16::MAX as u64) {
        return None;
    }
    sigs.sort_unstable();
    let hash_bytes = if precision == 32 { 4 } else { 8 };
    let mut out = Vec::with_capacity(sigs.len() * (hash_bytes + 6));
    for &(h, d, s) in sigs.iter() {
        out.extend_from_slice(&h.to_le_bytes()[..hash_bytes]);
        out.extend_from_slice(&(d as u32).to_le_bytes());
        out.extend_from_slice(&(s as u16).to_le_bytes());
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::minhash::HashFn;

    fn signer(n: usize, split: bool) -> Signer {
        let hash = HashConfig { hash_fn: HashFn::Xxhash, precision: 64 };
        Signer::new(n, split, NormConfig::default(), hash, Words::default())
    }

    #[test]
    fn windows_skip_blank_grams() {
        let mut out = Vec::new();
        assert!(signer(2, false).hashes("a\n\n\n\nb", &mut out));
        assert_eq!(out.iter().map(|&(_, i)| i).collect::<Vec<_>>(), [0, 3]);
        assert_eq!(out[0].0, xxhash_rust::xxh64::xxh64(b"a ", 0));
    }

    #[test]
    fn too_few_sentences() {
        let mut out = vec![(1, 1)];
        assert!(signer(3, true).hashes("One sentence. Two sentences.", &mut out));
        assert!(out.is_empty());
    }

    #[test]
    fn records_sort_and_overflow() {
        let mut sigs = vec![(5, 1, 2), (3, 9, 0), (5, 0, 7)];
        let r = sorted_records(&mut sigs, 32).unwrap();
        assert_eq!(r.len(), 30);
        assert_eq!(&r[..10], &[3, 0, 0, 0, 9, 0, 0, 0, 0, 0]);
        assert_eq!(&r[10..14], &[5, 0, 0, 0]);
        assert_eq!(&r[14..18], &[0, 0, 0, 0]);
        assert!(sorted_records(&mut [(1, 0, 65_536)], 64).is_none());
        assert!(sorted_records(&mut [(1 << 32, 0, 0)], 32).is_none());
    }
}
