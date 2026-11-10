//! `find_all_duplicate`: n-grams are keyed by `"".join(words[i:i+n])`, so distinct word splits can collide.
//!
//! Prefilter: two equal keys have equal byte length L; if L >= PREFIX both start with the same PREFIX bytes of
//! the joined text at a word boundary. So a window of length >= PREFIX whose start's PREFIX bytes hash to a
//! sketch slot no other word start hits is unique among all windows of its n: it is never a duplicate and
//! inserting it into the seen-set can never match a later window, so it is skipped. Slot collisions only add
//! candidates; candidates are hashed and compared by content.

use super::joined::Joined;
use hashbrown::HashTable;
use rustc_hash::FxHashSet;

const PREFIX: usize = 16;
const SKETCH_BITS: u32 = 16;

#[derive(Default)]
pub(super) struct DupNGrams {
    /// Per word start: windows of up to this many words are candidates (u32::MAX: all, its prefix is shared).
    max_candidate_n: Vec<u32>,
    /// Per sketch slot: word starts whose prefix hashes there (saturating).
    sketch: Vec<u8>,
    /// Sketch slot per word start with a full prefix.
    slots: Vec<u16>,
    short_seen: FxHashSet<u128>,
    /// Start positions of seen keys of PREFIX or more bytes.
    seen: HashTable<u32>,
}

impl DupNGrams {
    pub(super) fn prepare(&mut self, j: &Joined) {
        let Self { max_candidate_n: cand, sketch, slots, .. } = self;
        let n_words = j.n_words();
        let total = j.start(n_words);
        cand.clear();
        slots.clear();
        sketch.resize(1 << SKETCH_BITS, 0);
        let mut end = 0;
        for i in 0..n_words {
            // end: one past the last word w such that "".join(words[i..=w]) is shorter than PREFIX.
            end = end.max(i);
            while end < n_words && j.start(end + 1) - j.start(i) < PREFIX {
                end += 1;
            }
            cand.push((end - i) as u32);
            let a = j.start(i);
            if a + PREFIX <= total {
                let s = (j.word_hash(a, PREFIX) >> (64 - SKETCH_BITS)) as usize;
                sketch[s] = sketch[s].saturating_add(1);
                slots.push(s as u16);
            }
        }
        // Word starts with a full prefix are a leading run of positions, so slots[i] belongs to word i.
        for (i, &s) in slots.iter().enumerate() {
            if sketch[s as usize] > 1 {
                cand[i] = u32::MAX;
            }
        }
        for &s in slots.iter() {
            sketch[s as usize] = 0;
        }
    }

    /// `find_all_duplicate(words, n)` for the words last passed to `prepare`.
    pub(super) fn duplicate_chars(&mut self, j: &Joined, n: usize) -> usize {
        let n_words = j.n_words();
        if n == 0 || n_words < n {
            return 0;
        }
        let count = n_words - n + 1;
        let Self { max_candidate_n, short_seen, seen, .. } = self;
        let cand = &max_candidate_n[..count];
        // clear() touches every bucket, so drop capacity a past huge doc left behind.
        if seen.capacity() > 4 * count.max(1024) {
            (*short_seen, *seen) = Default::default();
        }
        short_seen.clear();
        seen.clear();
        let (mut repeated, mut idx) = (0, 0);
        while let Some(skip) = cand[idx..].iter().position(|&m| m as usize >= n) {
            idx += skip;
            let (a, len) = (j.start(idx), j.start(idx + n) - j.start(idx));
            // Keys shorter than PREFIX are compared by their exact u128 form; they never equal longer ones.
            let is_dup = if len < PREFIX {
                !short_seen.insert(j.short_key(a, len))
            } else {
                let (s, h) = (j.span(idx, n), j.word_hash(a, len));
                let found = seen.find(h, |&i| j.span(i as usize, n) == s).is_some();
                if !found {
                    seen.insert_unique(h, idx as u32, |&i| {
                        j.word_hash(j.start(i as usize), j.span(i as usize, n).len())
                    });
                }
                found
            };
            if is_dup {
                repeated += j.chars(idx, n);
                idx += n;
            } else {
                idx += 1;
            }
            if idx >= count {
                break;
            }
        }
        repeated
    }
}
