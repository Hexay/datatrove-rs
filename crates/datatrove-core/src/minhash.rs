//! `MinhashDedupSignature.get_shingles` / `get_signature`.

pub use crate::ngrams::{HashConfig, HashFn, Shingler as MinHasher};

const MERSENNE_PRIME: u64 = (1 << 61) - 1;

/// `(s * a + b) % (2**61 - 1)` as numpy computes it in uint64: the multiply and add wrap at 2**64 first.
#[inline(always)]
pub fn permute(s: u64, a: u64, b: u64) -> u64 {
    mod_mersenne(s.wrapping_mul(a).wrapping_add(b))
}

/// Per-hash minimum of `permute(s, a, b)` over non-empty `shingles`; 32-bit precision masks each value before the
/// min.
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
            *o = (*o).min(permute(s, a, b) & mask);
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
