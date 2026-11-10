//! `datatrove.pipeline.filters.GopherRepetitionFilter.filter`.

use super::{div, drop, on, FilterResult, Verdict};
use crate::pystr::{char_len, strip};
use crate::words::{joined_len, Words};
use rustc_hash::{FxHashMap, FxHashSet};
use std::borrow::Cow;

pub struct GopherRepetitionConfig {
    pub dup_line_frac: Option<f64>,
    pub dup_para_frac: Option<f64>,
    pub dup_line_char_frac: Option<f64>,
    pub dup_para_char_frac: Option<f64>,
    pub top_n_grams: Vec<(usize, f64)>,
    pub dup_n_grams: Vec<(usize, f64)>,
}

pub struct GopherRepetition {
    cfg: GopherRepetitionConfig,
    words: Words,
}

/// `find_duplicates`: (duplicate element count, their total char length).
pub fn find_duplicates<'a>(items: impl IntoIterator<Item = &'a str>) -> (usize, usize) {
    let mut seen = FxHashSet::default();
    let (mut elements, mut chars) = (0, 0);
    for x in items {
        if !seen.insert(x) {
            elements += 1;
            chars += char_len(x);
        }
    }
    (elements, chars)
}

/// `re.split(r"\n{min,}", s)` (min = 1 is `\n+`): pieces between maximal newline runs of at least `min`.
fn split_newline_runs(s: &str, min: usize) -> Vec<&str> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let (mut start, mut i) = (0, 0);
    while let Some(off) = s[i..].find('\n') {
        let a = i + off;
        let mut e = a;
        while e < b.len() && b[e] == b'\n' {
            e += 1;
        }
        if e - a >= min {
            out.push(&s[start..a]);
            start = e;
        }
        i = e;
    }
    out.push(&s[start..]);
    out
}

/// Word -> dense id, so n-grams can be keyed by id windows (joined with " ", word sequences are injective).
fn intern(words: &[&str]) -> Vec<u32> {
    let mut ids: FxHashMap<&str, u32> = FxHashMap::default();
    words
        .iter()
        .map(|w| {
            let next = ids.len() as u32;
            *ids.entry(w).or_insert(next)
        })
        .collect()
}

/// `find_top_duplicate(get_n_grams(words, n))`: `len(top) * count` of `Counter.most_common(1)`,
/// whose ties go to the n-gram seen first. `None` when there are no n-grams.
fn top_duplicate_chars(ids: &[u32], words: &[&str], n: usize) -> Option<usize> {
    // n = 0 yields len(words) + 1 empty n-grams, so the top one has zero chars.
    if n == 0 {
        return Some(0);
    }
    if ids.len() < n {
        return None;
    }
    let distinct = ids.iter().max().map_or(0, |&m| m + 1);
    let (i, max) = if n <= 4 && distinct <= 1 << 16 {
        first_most_common(ids.windows(n).map(|g| g.iter().fold(0u64, |k, &id| k << 16 | id as u64)))
    } else {
        first_most_common(ids.windows(n))
    };
    Some(joined_len(&words[i..i + n], 1) * max)
}

/// (index of the first key with the highest count, that count) — `Counter.most_common(1)` tie-breaking.
fn first_most_common<K: std::hash::Hash + Eq + Copy>(mut keys: impl Iterator<Item = K> + Clone) -> (usize, usize) {
    let mut counts: FxHashMap<K, usize> = FxHashMap::default();
    for k in keys.clone() {
        *counts.entry(k).or_default() += 1;
    }
    let max = counts.values().copied().max().unwrap_or(0);
    (keys.position(|k| counts[&k] == max).unwrap_or(0), max)
}

const M61: u64 = (1 << 61) - 1;
const BASE: u64 = 0x1d8e_4e27_c47d_124f % M61;

#[inline]
fn mulmod61(a: u64, b: u64) -> u64 {
    let p = a as u128 * b as u128;
    let r = (p as u64 & M61) + (p >> 61) as u64;
    if r >= M61 {
        r - M61
    } else {
        r
    }
}

/// `"".join(words)` with prefix char counts and prefix polynomial hashes, so the `"".join(words[i:i+n])` key
/// `find_all_duplicate` uses is a contiguous slice whose hash costs O(1).
struct Concat {
    text: String,
    byte_off: Vec<usize>,
    char_off: Vec<usize>,
    prefix: Vec<u64>,
    pow: Vec<u64>,
}

impl Concat {
    fn new(words: &[&str]) -> Self {
        let text: String = words.concat();
        let (mut byte_off, mut char_off) = (vec![0], vec![0]);
        for w in words {
            byte_off.push(byte_off.last().unwrap() + w.len());
            char_off.push(char_off.last().unwrap() + char_len(w));
        }
        let (mut prefix, mut pow) = (Vec::with_capacity(text.len() + 1), Vec::with_capacity(text.len() + 1));
        prefix.push(0);
        pow.push(1);
        for &b in text.as_bytes() {
            let (h, p) = (*prefix.last().unwrap(), *pow.last().unwrap());
            prefix.push((mulmod61(h, BASE) + b as u64 + 1) % M61);
            pow.push(mulmod61(p, BASE));
        }
        Self { text, byte_off, char_off, prefix, pow }
    }

    /// Key of `"".join(words[i:i+n])`.
    fn key(&self, i: usize, n: usize) -> SliceKey<'_> {
        let (a, b) = (self.byte_off[i], self.byte_off[i + n]);
        let hash = (self.prefix[b] + M61 - mulmod61(self.prefix[a], self.pow[b - a])) % M61;
        SliceKey { hash, s: &self.text[a..b] }
    }
}

/// Hashes by the precomputed polynomial hash, compares by content, so collisions cannot change results.
struct SliceKey<'a> {
    hash: u64,
    s: &'a str,
}

impl PartialEq for SliceKey<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.hash == other.hash && self.s == other.s
    }
}

impl Eq for SliceKey<'_> {}

impl std::hash::Hash for SliceKey<'_> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        state.write_u64(self.hash);
    }
}

/// `find_all_duplicate`: n-grams are keyed by `"".join(...)`, so distinct word splits can collide.
fn all_duplicate_chars(concat: &Concat, n_words: usize, n: usize) -> usize {
    if n == 0 || n_words < n {
        return 0;
    }
    let count = n_words - n + 1;
    let mut unique = FxHashSet::with_capacity_and_hasher(count, Default::default());
    let (mut repeated, mut idx) = (0, 0);
    while idx < count {
        if unique.insert(concat.key(idx, n)) {
            idx += 1;
        } else {
            repeated += concat.char_off[idx + n] - concat.char_off[idx];
            idx += n;
        }
    }
    repeated
}

impl GopherRepetition {
    pub fn new(cfg: GopherRepetitionConfig) -> Self {
        Self { cfg, words: Words::default() }
    }

    pub fn filter(&mut self, text: &str) -> FilterResult {
        if text.is_empty() {
            return drop("empty");
        }
        let c = &self.cfg;
        let text_len = char_len(text);

        let paragraphs = split_newline_runs(strip(text), 2);
        let (para_dups, char_dups) = find_duplicates(paragraphs.iter().copied());
        if on(c.dup_para_frac).is_some_and(|m| para_dups as f64 / paragraphs.len() as f64 > m) {
            return drop("dup_para_frac");
        }
        if let Some(m) = on(c.dup_para_char_frac) {
            if div(char_dups, text_len)? > m {
                return drop("dup_para_char_frac");
            }
        }

        let lines = split_newline_runs(text, 1);
        let (line_dups, char_dups) = find_duplicates(lines.iter().copied());
        if on(c.dup_line_frac).is_some_and(|m| line_dups as f64 / lines.len() as f64 > m) {
            return drop("dup_line_frac");
        }
        if let Some(m) = on(c.dup_line_char_frac) {
            if div(char_dups, text_len)? > m {
                return drop("dup_line_char_frac");
            }
        }

        let mut words = Vec::new();
        self.words.words(text, &mut words);
        if !c.top_n_grams.is_empty() {
            let ids = intern(&words);
            for &(n, frac) in &c.top_n_grams {
                let Some(top) = top_duplicate_chars(&ids, &words, n) else { continue };
                if div(top, text_len)? > frac {
                    return Ok(Verdict::Drop(Cow::Owned(format!("top_{n}_gram"))));
                }
            }
        }

        if c.dup_n_grams.is_empty() {
            return Ok(Verdict::Keep);
        }
        let concat = Concat::new(&words);
        for &(n, frac) in &c.dup_n_grams {
            if div(all_duplicate_chars(&concat, words.len(), n), text_len)? > frac {
                return Ok(Verdict::Drop(Cow::Owned(format!("duplicated_{n}_n_grams"))));
            }
        }

        Ok(Verdict::Keep)
    }
}
