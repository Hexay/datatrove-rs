//! Hashed word n-grams of simplified text, as MinHash, the Bloom filter and n-gram decontamination compute them:
//! `hash_fc(" ".join(g)) for g in ngrams(word_tokenize(simplify_text(text, norm)), n)`.

use crate::simplify::{NormConfig, Simplifier};
use crate::words::Words;
use std::ops::ControlFlow;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum HashFn {
    Xxhash,
    Sha1,
}

#[derive(Clone, Copy)]
pub struct HashConfig {
    pub hash_fn: HashFn,
    /// 32 or 64.
    pub precision: u32,
}

impl HashConfig {
    pub fn hash(&self, s: &str) -> u64 {
        let b = s.as_bytes();
        match (self.hash_fn, self.precision) {
            (HashFn::Xxhash, 32) => xxhash_rust::xxh32::xxh32(b, 0) as u64,
            (HashFn::Xxhash, _) => xxhash_rust::xxh64::xxh64(b, 0),
            (HashFn::Sha1, p) => {
                let d = sha1_smol::Sha1::from(b).digest().bytes();
                if p == 32 {
                    u32::from_le_bytes(d[..4].try_into().unwrap()) as u64
                } else {
                    u64::from_le_bytes(d[..8].try_into().unwrap())
                }
            }
        }
    }
}

pub struct Shingler {
    n_grams: usize,
    hash: HashConfig,
    simplifier: Simplifier,
    words: Words,
    joined: String,
}

impl Shingler {
    pub fn new(n_grams: usize, norm: NormConfig, hash: HashConfig, words: Words) -> Self {
        Self { n_grams, hash, simplifier: Simplifier::new(norm), words, joined: String::new() }
    }

    /// Hashes of `" ".join(ngram)` over the word tokens of the simplified text, in order.
    pub fn shingles(&mut self, text: &str, out: &mut Vec<u64>) {
        out.clear();
        let _ = self.each(text, |_, h| {
            out.push(h);
            ControlFlow::<()>::Continue(())
        });
    }

    /// The first n-gram (joined text, hash) whose hash `hit` accepts.
    pub fn find(&mut self, text: &str, mut hit: impl FnMut(u64) -> bool) -> Option<(String, u64)> {
        match self
            .each(text, |g, h| if hit(h) { ControlFlow::Break((g.to_owned(), h)) } else { ControlFlow::Continue(()) })
        {
            ControlFlow::Break(found) => Some(found),
            ControlFlow::Continue(()) => None,
        }
    }

    /// `NGramsDecontIndexer.compute_hashes(label, query)`: the label's n-grams, then the query's (when
    /// `find_query`), then those spanning the query's tail and the label's head (when `find_overlap`).
    pub fn decont_hashes(
        &mut self,
        label: &str,
        query: Option<&str>,
        find_query: bool,
        find_overlap: bool,
    ) -> Vec<u64> {
        let n = self.n_grams;
        let label_tokens = self.tokens(label);
        let query_tokens = query.map(|q| self.tokens(q));
        if n == 0 {
            return Vec::new();
        }
        let mut grams: Vec<Vec<&String>> = label_tokens.windows(n).map(|g| g.iter().collect()).collect();
        if let Some(query_tokens) = &query_tokens {
            if find_query {
                grams.extend(query_tokens.windows(n).map(|g| g.iter().collect()));
            }
            if find_overlap {
                for i in 0..n - 1 {
                    let from_query = n - 1 - i;
                    if query_tokens.len() >= from_query && label_tokens.len() > i {
                        let tail = &query_tokens[query_tokens.len() - from_query..];
                        grams.push(tail.iter().chain(&label_tokens[..=i]).collect());
                    }
                }
            }
        }
        let hash = self.hash;
        grams.iter().map(|g| hash.hash(&g.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(" "))).collect()
    }

    fn tokens(&mut self, text: &str) -> Vec<String> {
        let simple = self.simplifier.simplify(text);
        let mut words = Vec::new();
        self.words.words(simple, &mut words);
        words.into_iter().map(str::to_owned).collect()
    }

    fn each<B>(&mut self, text: &str, mut f: impl FnMut(&str, u64) -> ControlFlow<B>) -> ControlFlow<B> {
        let simple = self.simplifier.simplify(text);
        let mut words = Vec::new();
        self.words.words(simple, &mut words);
        if self.n_grams == 0 || words.len() < self.n_grams {
            return ControlFlow::Continue(());
        }
        for g in words.windows(self.n_grams) {
            self.joined.clear();
            for (i, w) in g.iter().enumerate() {
                if i > 0 {
                    self.joined.push(' ');
                }
                self.joined.push_str(w);
            }
            f(&self.joined, self.hash.hash(&self.joined))?;
        }
        ControlFlow::Continue(())
    }
}
