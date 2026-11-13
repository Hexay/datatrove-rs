//! `NGramsDecontFilter.filter`'s n-gram lookup against the eval-set index.

use crate::ngrams::Shingler;
use rustc_hash::FxHashSet;

pub struct Decont {
    shingler: Shingler,
    index: FxHashSet<u64>,
}

impl Decont {
    pub fn new(shingler: Shingler) -> Self {
        Self { shingler, index: FxHashSet::default() }
    }

    pub fn set_index(&mut self, hashes: impl IntoIterator<Item = u64>) {
        self.index = hashes.into_iter().collect();
    }

    /// The first n-gram of `text` (joined text, hash) whose hash is in the index.
    pub fn find(&mut self, text: &str) -> Option<(String, u64)> {
        let index = &self.index;
        self.shingler.find(text, |h| index.contains(&h))
    }

    /// `NGramsDecontIndexer.compute_hashes`; see `Shingler::decont_hashes`.
    pub fn compute_hashes(
        &mut self,
        label: &str,
        query: Option<&str>,
        find_query: bool,
        find_overlap: bool,
    ) -> Vec<u64> {
        self.shingler.decont_hashes(label, query, find_query, find_overlap)
    }
}
