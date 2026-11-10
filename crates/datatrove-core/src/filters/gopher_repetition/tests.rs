//! Randomized checks of the n-gram fast paths against direct ports of datatrove's functions.

use super::{dup::DupNGrams, joined::Joined, top::TopNGrams};
use std::collections::{HashMap, HashSet};

/// `find_top_duplicate(get_n_grams(words, n))`, with `Counter` insertion order kept explicitly.
fn py_top(words: &[&str], n: usize) -> Option<usize> {
    let grams: Vec<String> = (0..(words.len() + 1).saturating_sub(n)).map(|i| words[i..i + n].join(" ")).collect();
    let mut order: Vec<&String> = Vec::new();
    let mut counts: HashMap<&String, usize> = HashMap::new();
    for g in &grams {
        *counts.entry(g).or_insert_with(|| {
            order.push(g);
            0
        }) += 1;
    }
    let mut best: Option<(&String, usize)> = None;
    for g in order {
        if best.is_none_or(|(_, c)| counts[g] > c) {
            best = Some((g, counts[g]));
        }
    }
    best.map(|(g, c)| g.chars().count() * c)
}

fn py_all_duplicate(words: &[&str], n: usize) -> usize {
    let mut unique = HashSet::new();
    let (mut repeated, mut idx) = (0, 0);
    while idx + n < words.len() + 1 {
        let g = words[idx..idx + n].concat();
        if unique.contains(&g) {
            repeated += g.chars().count();
            idx += n;
        } else {
            unique.insert(g);
            idx += 1;
        }
    }
    repeated
}

const VOCAB: &[&str] = &[
    "a", "b", "ab", "ba", "aa", "é", "aé", ".", "the", "of", "ofthe", "thecat", "cat", "abcdefghijklmnop",
    "abcdefghijklmnopq", "abcdefghijklmno", "abcdefghijklmnopabcdefghijklmnop", "ééééééééé", "x\0", "\0",
];

#[test]
fn ngram_fast_paths_match_reference() {
    let mut seed = 0x9e37_79b9_7f4a_7c15u64;
    let mut rnd = |m: usize| {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (seed >> 33) as usize % m
    };
    let (mut j, mut top, mut dup) = (Joined::default(), TopNGrams::default(), DupNGrams::default());
    for _ in 0..3000 {
        let vocab = 1 + rnd(VOCAB.len());
        let words: Vec<&str> = (0..rnd(60)).map(|_| VOCAB[rnd(vocab)]).collect();
        j.build(&words);
        top.prepare(&j);
        dup.prepare(&j);
        for n in 0..12 {
            let want = py_top(&words, n);
            if let (Some(b), Some(w)) = (top.bound(&j, n), want) {
                assert!(b >= w, "bound {words:?} n={n}");
            }
            assert_eq!(top.top_chars(&j, n), want, "top {words:?} n={n}");
            if n > 0 {
                assert_eq!(dup.duplicate_chars(&j, n), py_all_duplicate(&words, n), "dup {words:?} n={n}");
            }
        }
    }
}
