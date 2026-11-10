//! `MinhashDedupSignature.get_shingles` / `get_signature`.

use crate::simplify::{NormConfig, Simplifier};
use crate::words::Words;

const MERSENNE_PRIME: u64 = (1 << 61) - 1;

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
    fn hash(&self, s: &str) -> u64 {
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

pub struct MinHasher {
    n_grams: usize,
    hash: HashConfig,
    simplifier: Simplifier,
    words: Words,
    joined: String,
}

impl MinHasher {
    pub fn new(n_grams: usize, norm: NormConfig, hash: HashConfig) -> Self {
        Self { n_grams, hash, simplifier: Simplifier::new(norm), words: Words::default(), joined: String::new() }
    }

    /// Hashes of `" ".join(ngram)` over the word tokens of the simplified text, in order.
    pub fn shingles(&mut self, text: &str, out: &mut Vec<u64>) {
        out.clear();
        let simple = self.simplifier.simplify(text);
        let mut words = Vec::new();
        self.words.words(simple, &mut words);
        if self.n_grams == 0 || words.len() < self.n_grams {
            return;
        }
        for g in words.windows(self.n_grams) {
            self.joined.clear();
            for (i, w) in g.iter().enumerate() {
                if i > 0 {
                    self.joined.push(' ');
                }
                self.joined.push_str(w);
            }
            out.push(self.hash.hash(&self.joined));
        }
    }
}

/// Per-hash minimum of `(s * a + b) % P` over non-empty `shingles`. numpy does this in uint64, so the
/// multiply and add wrap at 2**64 before the modulo; 32-bit precision masks each value before the min.
pub fn signature(shingles: &[u64], a: &[u64], b: &[u64], precision: u32, out: &mut Vec<u64>) {
    out.clear();
    out.resize(a.len().min(b.len()), u64::MAX);
    let mask = if precision == 32 { u32::MAX as u64 } else { u64::MAX };
    #[cfg(target_arch = "x86_64")]
    if std::is_x86_feature_detected!("avx512dq") {
        // SAFETY: feature checked above.
        return unsafe { signature_avx512(shingles, a, b, mask, out) };
    }
    signature_into(shingles, a, b, mask, out);
}

/// Shingle-outer, permutation-inner so the inner loop vectorizes across permutations.
#[inline(always)]
fn signature_into(shingles: &[u64], a: &[u64], b: &[u64], mask: u64, out: &mut [u64]) {
    let (a, b) = (&a[..out.len()], &b[..out.len()]);
    for &s in shingles {
        for ((o, &a), &b) in out.iter_mut().zip(a).zip(b) {
            *o = (*o).min(mod_mersenne(s.wrapping_mul(a).wrapping_add(b)) & mask);
        }
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx512f,avx512dq,avx512vl")]
unsafe fn signature_avx512(shingles: &[u64], a: &[u64], b: &[u64], mask: u64, out: &mut [u64]) {
    signature_into(shingles, a, b, mask, out)
}

/// `x % (2**61 - 1)` for any u64, via 2**61 ≡ 1: the folded sum is < 2**61 + 8, so one subtract suffices.
/// Branchless: when `r < P` the subtraction wraps to a huge value and `min` keeps `r`.
#[inline(always)]
fn mod_mersenne(x: u64) -> u64 {
    let r = (x & MERSENNE_PRIME) + (x >> 61);
    r.min(r.wrapping_sub(MERSENNE_PRIME))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mod_mersenne_matches_remainder() {
        let edge =
            [0, 1, MERSENNE_PRIME - 1, MERSENNE_PRIME, MERSENNE_PRIME + 1, 2 * MERSENNE_PRIME, u64::MAX, u64::MAX - 7];
        let mut x = 0x9e37_79b9_7f4a_7c15u64;
        let random = (0..1_000_000).map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x
        });
        for v in edge.into_iter().chain(random) {
            assert_eq!(mod_mersenne(v), v % MERSENNE_PRIME, "{v}");
        }
    }
}
