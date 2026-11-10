//! `find_top_duplicate(get_n_grams(words, n))`, with buffers reused across docs.

use super::joined::Joined;
use hashbrown::HashTable;
use rustc_hash::{FxBuildHasher, FxHashMap};
use std::hash::{BuildHasher, Hash};

const SKETCH_BITS: u32 = 12;

#[derive(Default)]
pub(super) struct TopNGrams {
    /// Per sketch slot: occurrences of the bigrams hashed there.
    sketch: Vec<u32>,
    /// Per bigram position: its sketch slot, and that slot's count (>= the bigram's count).
    slots: Vec<u16>,
    bigram_count: Vec<u32>,
    min_count: Vec<u32>,
    min_n: usize,
    interned: bool,
    /// Word id per position; ids are dense and numbered in first-seen order.
    ids: Vec<u32>,
    /// Per word id: position of its first occurrence and occurrence count.
    first: Vec<u32>,
    counts: Vec<u32>,
    /// Interners for words shorter than 16 bytes and the rest.
    short: FxHashMap<u128, u32>,
    long: HashTable<u32>,
    packed: FxHashMap<u64, (u32, u32)>,
}

impl TopNGrams {
    /// Fills `bigram_count` from a count sketch of bigrams.
    pub(super) fn prepare(&mut self, j: &Joined) {
        let Self { sketch, slots, bigram_count, .. } = self;
        sketch.resize(1 << SKETCH_BITS, 0);
        slots.clear();
        let off = j.offsets();
        let mut prev = 0u64;
        for (i, (&a, &b)) in off.iter().zip(&off[1..]).enumerate() {
            let cur = j.word_hash(a as usize, (b - a) as usize);
            if i > 0 {
                let s = ((prev.rotate_left(23) ^ cur).wrapping_mul(0x9e37_79b9_7f4a_7c15) >> (64 - SKETCH_BITS)) as u16;
                sketch[s as usize] += 1;
                slots.push(s);
            }
            prev = cur;
        }
        bigram_count.clear();
        bigram_count.extend(slots.iter().map(|&s| sketch[s as usize]));
        for &s in slots.iter() {
            sketch[s as usize] = 0;
        }
        self.min_n = 0;
        self.interned = false;
    }

    /// Upper bound on the top n-gram's `len * count`, `None` when not cheap (n < 2 or n > 16). An n-gram occurs
    /// at most as often as each of its bigrams, so at each position its count is <= the min of its bigrams'
    /// sketch counts (collisions only add); its byte length plus n - 1 separators is >= its char length.
    pub(super) fn bound(&mut self, j: &Joined, n: usize) -> Option<usize> {
        if !(2..=16).contains(&n) || j.n_words() < n {
            return None;
        }
        // min_count[i] = min(bigram_count[i..i + min_n - 1]); extended one n at a time, as configs ascend.
        if self.min_n + 1 != n {
            self.min_count.clone_from(&self.bigram_count);
            self.min_n = 2;
        }
        while self.min_n < n {
            let next = &self.bigram_count[self.min_n - 1..];
            self.min_count.truncate(next.len());
            self.min_count.iter_mut().zip(next).for_each(|(m, &c)| *m = (*m).min(c));
            self.min_n += 1;
        }
        let off = j.offsets();
        let lens = off[n..].iter().zip(off).map(|(&b, &a)| b - a + n as u32 - 1);
        self.min_count.iter().zip(lens).map(|(&c, l)| c as usize * l as usize).max()
    }

    fn intern(&mut self, j: &Joined) {
        let Self { ids, first, counts, short, long, .. } = self;
        ids.clear();
        first.clear();
        counts.clear();
        short.clear();
        long.clear();
        for i in 0..j.n_words() {
            let next = first.len() as u32;
            let id = if j.start(i + 1) - j.start(i) < 16 {
                *short.entry(j.word_key(i)).or_insert(next)
            } else {
                let w = j.span(i, 1);
                let h = FxBuildHasher.hash_one(w);
                let found = long.find(h, |&id| j.span(first[id as usize] as usize, 1) == w).copied();
                found.unwrap_or_else(|| {
                    long.insert_unique(h, next, |&id| FxBuildHasher.hash_one(j.span(first[id as usize] as usize, 1)));
                    next
                })
            };
            if id == next {
                first.push(i as u32);
                counts.push(0);
            }
            counts[id as usize] += 1;
            ids.push(id);
        }
        self.interned = true;
    }

    /// `len(top) * count` of `Counter(get_n_grams(words, n)).most_common(1)`, `None` when there are no n-grams.
    pub(super) fn top_chars(&mut self, j: &Joined, n: usize) -> Option<usize> {
        // n = 0 yields len(words) + 1 empty n-grams, so the top one has zero chars.
        if n == 0 {
            return Some(0);
        }
        if j.n_words() < n {
            return None;
        }
        if !self.interned {
            self.intern(j);
        }
        let (count, at) = if n == 1 {
            // Ids are in first-seen order, so the first max id is Counter's tie winner.
            let (id, &c) = self.counts.iter().enumerate().fold((0, &0), |b, x| if x.1 > b.1 { x } else { b });
            (c, self.first[id])
        } else if n <= 8 && self.counts.len() as u64 <= 1 << (64 / n) {
            let bits = 64 / n;
            self.packed.clear();
            let keys = self.ids.windows(n).map(|g| g.iter().fold(0u64, |k, &id| k << bits | id as u64));
            first_most_common(&mut self.packed, keys)
        } else {
            first_most_common(&mut FxHashMap::default(), self.ids.windows(n))
        };
        Some((j.chars(at as usize, n) + n - 1) * count as usize)
    }
}

/// (top count, position of its first occurrence). `Counter.most_common(1)` takes the first max in insertion
/// order, i.e. the highest count with the earliest first occurrence; counts only grow, so tracking it online is exact.
fn first_most_common<K: Hash + Eq>(map: &mut FxHashMap<K, (u32, u32)>, keys: impl ExactSizeIterator<Item = K>) -> (u32, u32) {
    map.reserve(keys.len());
    let mut best = (0, u32::MAX);
    for (i, k) in keys.enumerate() {
        let e = map.entry(k).or_insert((0, i as u32));
        e.0 += 1;
        if e.0 > best.0 || (e.0 == best.0 && e.1 < best.1) {
            best = *e;
        }
    }
    best
}
