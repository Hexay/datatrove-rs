//! Special cases (`tokenizer.rules`) and the PhraseMatcher pass of `_apply_special_cases`.

use hashbrown::HashTable;
use rustc_hash::FxHashMap;
use std::hash::BuildHasher;

pub type Span = (u32, u32);

/// String -> token byte lengths, looked up by a caller-computed hash so one hash serves several tables.
#[derive(Default)]
pub struct LensTable(HashTable<(Box<str>, Box<[u32]>)>);

#[inline]
pub fn hash_str(s: &str) -> u64 {
    rustc_hash::FxBuildHasher.hash_one(s)
}

impl LensTable {
    #[inline]
    pub fn get(&self, h: u64, s: &str) -> Option<&[u32]> {
        self.0.find(h, |e| &*e.0 == s).map(|e| &*e.1)
    }

    /// Caller guarantees `s` is absent.
    pub fn insert_new(&mut self, h: u64, s: &str, lens: Box<[u32]>) {
        self.0.insert_unique(h, (s.into(), lens), |e| hash_str(&e.0));
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// chunk -> byte lengths of its ORTH pieces (ORTHs concatenate to the chunk; spaCy validates this).
#[derive(Default)]
pub struct Specials(LensTable);

impl Specials {
    pub fn new(rules: &[(String, Vec<String>)]) -> Result<Self, String> {
        let mut t = LensTable::default();
        for (chunk, orths) in rules {
            if orths.concat() != *chunk {
                return Err(format!("special case {chunk:?}: ORTHs do not concatenate to chunk"));
            }
            let h = hash_str(chunk);
            let lens: Box<[u32]> = orths.iter().map(|o| o.len() as u32).collect();
            match t.0.find_mut(h, |e| *e.0 == **chunk) {
                Some(e) => e.1 = lens,
                None => t.insert_new(h, chunk, lens),
            }
        }
        Ok(Self(t))
    }

    #[inline]
    pub fn get_hashed(&self, h: u64, s: &str) -> Option<&[u32]> {
        self.0.get(h, s)
    }

    #[inline]
    pub fn get(&self, s: &str) -> Option<&[u32]> {
        self.0.get(hash_str(s), s)
    }

    #[inline]
    pub fn contains(&self, s: &str) -> bool {
        self.get(s).is_some()
    }
}

/// Token-ORTH trie equivalent to `PhraseMatcher.find_matches` over the special-case patterns.
pub struct PhraseTrie {
    children: Vec<FxHashMap<Box<str>, u32>>,
    terminal: Vec<bool>,
    /// Bit per (first byte, last byte) of the root's child tokens: rejects nearly every token without hashing.
    root_edges: Box<[u64; 1024]>,
}

impl Default for PhraseTrie {
    fn default() -> Self {
        Self::new()
    }
}

#[inline]
fn edge_key(tok: &str) -> Option<usize> {
    let b = tok.as_bytes();
    Some((*b.first()? as usize) << 8 | *b.last()? as usize)
}

impl PhraseTrie {
    pub fn new() -> Self {
        Self { children: vec![FxHashMap::default()], terminal: vec![false], root_edges: Box::new([0; 1024]) }
    }

    #[inline]
    fn maybe_root(&self, tok: &str) -> bool {
        edge_key(tok).is_some_and(|k| self.root_edges[k >> 6] >> (k & 63) & 1 != 0)
    }

    pub fn add(&mut self, tokens: &[&str]) {
        let Some(k) = edge_key(tokens.first().copied().unwrap_or_default()) else { return };
        self.root_edges[k >> 6] |= 1 << (k & 63);
        let mut node = 0usize;
        for t in tokens {
            node = match self.children[node].get(*t) {
                Some(&n) => n as usize,
                None => {
                    let n = self.children.len();
                    self.children.push(FxHashMap::default());
                    self.terminal.push(false);
                    self.children[node].insert((*t).into(), n as u32);
                    n
                }
            };
        }
        self.terminal[node] = true;
    }

    pub fn len(&self) -> usize {
        self.terminal.iter().filter(|&&t| t).count()
    }

    pub fn is_empty(&self) -> bool {
        !self.terminal.contains(&true)
    }

    #[inline]
    fn child(&self, node: usize, tok: &str) -> Option<usize> {
        self.children[node].get(tok).map(|&n| n as usize)
    }

    /// All (start, end) token-index matches, like `find_matches` (every terminal on every walk).
    fn find(&self, text: &str, toks: &[Span], out: &mut Vec<(usize, usize)>) {
        out.clear();
        let text_of = |i: usize| &text[toks[i].0 as usize..toks[i].1 as usize];
        for start in 0..toks.len() {
            let first = text_of(start);
            if !self.maybe_root(first) {
                continue;
            }
            let Some(mut node) = self.child(0, first) else { continue };
            let mut end = start + 1;
            loop {
                if self.terminal[node] {
                    out.push((start, end));
                }
                if end == toks.len() {
                    break;
                }
                match self.child(node, text_of(end)) {
                    Some(n) => {
                        node = n;
                        end += 1;
                    }
                    None => break,
                }
            }
        }
    }

    /// `_apply_special_cases`: filter overlapping matches and retokenize spans whose text is a
    /// special case. Returns false (and leaves `toks` untouched) when nothing matched.
    pub fn apply(&self, specials: &Specials, text: &str, toks: &mut Vec<Span>, scratch: &mut Scratch) -> bool {
        self.find(text, toks, &mut scratch.matches);
        if scratch.matches.is_empty() {
            return false;
        }
        let m = &mut scratch.matches;
        // _filter_special_spans: longest first, then leftmost; a rejected span still marks its
        // tokens as seen, and only the endpoints are tested (both quirks are spaCy's).
        m.sort_by(|a, b| (b.1 - b.0).cmp(&(a.1 - a.0)).then(a.0.cmp(&b.0)));
        scratch.seen.clear();
        scratch.seen.resize(toks.len(), false);
        scratch.kept.clear();
        for &(s, e) in m.iter() {
            if !scratch.seen[s] && !scratch.seen[e - 1] {
                scratch.kept.push((s, e));
            }
            scratch.seen[s..e].iter_mut().for_each(|x| *x = true);
        }
        scratch.kept.sort_unstable();
        scratch.out.clear();
        let mut i = 0;
        for &(s, e) in &scratch.kept {
            scratch.out.extend_from_slice(&toks[i..s]);
            let (a, b) = (toks[s].0, toks[e - 1].1);
            match specials.get(&text[a as usize..b as usize]) {
                Some(lens) => {
                    let mut p = a;
                    for &l in lens {
                        scratch.out.push((p, p + l));
                        p += l;
                    }
                }
                None => scratch.out.extend_from_slice(&toks[s..e]),
            }
            i = e;
        }
        scratch.out.extend_from_slice(&toks[i..]);
        std::mem::swap(toks, &mut scratch.out);
        true
    }
}

#[derive(Default)]
pub struct Scratch {
    matches: Vec<(usize, usize)>,
    kept: Vec<(usize, usize)>,
    seen: Vec<bool>,
    out: Vec<Span>,
}
