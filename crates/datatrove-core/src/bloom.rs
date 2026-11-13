//! `SingleBloomFilter.step`: shingles, the k bit indexes per shingle, query, then set.

use crate::minhash::permute;
use crate::ngrams::Shingler;

/// Some bit index falls past the end of the bit vector, where datatrove raises IndexError.
#[derive(Debug)]
pub struct OutOfRange;

pub struct Bloom {
    shingler: Shingler,
    a: Vec<u64>,
    b: Vec<u64>,
    m_bytes: u64,
    shingles: Vec<u64>,
    fresh: Vec<u64>,
}

impl Bloom {
    /// `a`/`b` are `parameters` (k each); `m_bytes` is `config.m_bytes`.
    pub fn new(shingler: Shingler, a: Vec<u64>, b: Vec<u64>, m_bytes: u64) -> Self {
        Self { shingler, a, b, m_bytes, shingles: Vec::new(), fresh: Vec::new() }
    }

    /// Shingles `text` for the next `update`; returns how many.
    pub fn shingle(&mut self, text: &str) -> usize {
        self.shingler.shingles(text, &mut self.shingles);
        self.shingles.len()
    }

    /// Duplicate shingles among the last `shingle`'s, updating `bits` as datatrove does: every shingle is queried
    /// against the bits as they were before this doc, then the non-duplicates' bits are set.
    pub fn update(&mut self, bits: &mut [u8]) -> Result<usize, OutOfRange> {
        if self.shingles.is_empty() || self.a.is_empty() {
            return Ok(self.shingles.len());
        }
        // datatrove masks with `& m_bytes` (not `% m`), so every index is <= m_bytes.
        if (self.m_bytes >> 3) as usize >= bits.len() {
            return Err(OutOfRange);
        }
        self.fresh.clear();
        let mut dups = 0;
        for &s in &self.shingles {
            let start = self.fresh.len();
            let mut present = true;
            for (&a, &b) in self.a.iter().zip(&self.b) {
                let idx = permute(s, a, b) & self.m_bytes;
                present &= bits[(idx >> 3) as usize] & (1 << (idx & 7)) != 0;
                self.fresh.push(idx);
            }
            if present {
                dups += 1;
                self.fresh.truncate(start);
            }
        }
        for &idx in &self.fresh {
            bits[(idx >> 3) as usize] |= 1 << (idx & 7);
        }
        Ok(dups)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ngrams::{HashConfig, HashFn};
    use crate::simplify::NormConfig;
    use crate::words::Words;

    fn bloom(k: usize, m_bytes: u64) -> Bloom {
        let hash = HashConfig { hash_fn: HashFn::Xxhash, precision: 32 };
        let s = Shingler::new(1, NormConfig::default(), hash, Words::default());
        Bloom::new(s, (1..=k as u64).collect(), vec![7; k], m_bytes)
    }

    #[test]
    fn repeats_within_a_doc_are_not_duplicates_until_the_next_doc() {
        let mut b = bloom(3, 4095);
        let mut bits = vec![0u8; 4095];
        assert_eq!(b.shingle("cat cat cat"), 3);
        assert_eq!(b.update(&mut bits).unwrap(), 0);
        b.shingle("cat cat");
        assert_eq!(b.update(&mut bits).unwrap(), 2);
    }

    #[test]
    fn edge_configs() {
        let mut bits = vec![0u8; 4];
        let mut b = bloom(0, 4095);
        b.shingle("a b");
        assert_eq!(b.update(&mut bits).unwrap(), 2, "k=0: every shingle is a duplicate");
        let mut b = bloom(2, 4095);
        b.shingle("a b");
        assert!(b.update(&mut bits).is_err(), "indexes up to m_bytes need m_bytes / 8 + 1 bytes");
        assert_eq!(bits, [0; 4]);
    }
}
