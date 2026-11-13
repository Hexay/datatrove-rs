//! pythainlp's `Trie`: only `prefixes` is needed, and it yields matches shortest first.

use rustc_hash::FxHashMap;

pub struct Trie {
    children: FxHashMap<(u32, char), u32>,
    end: Vec<bool>,
}

impl Trie {
    pub fn new<'a>(words: impl Iterator<Item = &'a str>) -> Self {
        let mut t = Self { children: FxHashMap::default(), end: vec![false] };
        for w in words {
            let mut node = 0u32;
            for c in w.chars() {
                let next = t.end.len() as u32;
                node = *t.children.entry((node, c)).or_insert_with(|| next);
                if node == next {
                    t.end.push(false);
                }
            }
            t.end[node as usize] = true;
        }
        t
    }

    /// Lengths (in chars) of the dictionary words starting at `text[start]`, ascending.
    pub fn prefixes(&self, text: &[char], start: usize, out: &mut Vec<usize>) {
        out.clear();
        let mut node = 0u32;
        for (i, &c) in text[start..].iter().enumerate() {
            match self.children.get(&(node, c)) {
                Some(&n) => node = n,
                None => break,
            }
            if self.end[node as usize] {
                out.push(i + 1);
            }
        }
    }
}
