//! Special cases (`tokenizer.rules`) and the PhraseMatcher pass of `_apply_special_cases`.

use rustc_hash::FxHashMap;

pub type Span = (u32, u32);

/// chunk -> byte lengths of its ORTH pieces (ORTHs concatenate to the chunk; spaCy validates this).
#[derive(Default)]
pub struct Specials {
    map: FxHashMap<Box<str>, Box<[u32]>>,
}

impl Specials {
    pub fn new(rules: &[(String, Vec<String>)]) -> Result<Self, String> {
        let mut map = FxHashMap::default();
        for (chunk, orths) in rules {
            if orths.concat() != *chunk {
                return Err(format!("special case {chunk:?}: ORTHs do not concatenate to chunk"));
            }
            map.insert(chunk.as_str().into(), orths.iter().map(|o| o.len() as u32).collect());
        }
        Ok(Self { map })
    }

    #[inline]
    pub fn get(&self, s: &str) -> Option<&[u32]> {
        self.map.get(s).map(|v| &**v)
    }

    #[inline]
    pub fn contains(&self, s: &str) -> bool {
        self.map.contains_key(s)
    }
}

/// Token-ORTH trie equivalent to `PhraseMatcher.find_matches` over the special-case patterns.
#[derive(Default)]
pub struct PhraseTrie {
    children: Vec<FxHashMap<Box<str>, u32>>,
    terminal: Vec<bool>,
}

impl PhraseTrie {
    pub fn new() -> Self {
        Self { children: vec![FxHashMap::default()], terminal: vec![false] }
    }

    pub fn add(&mut self, tokens: &[&str]) {
        if tokens.is_empty() {
            return;
        }
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

    #[inline]
    fn child(&self, node: usize, tok: &str) -> Option<usize> {
        self.children[node].get(tok).map(|&n| n as usize)
    }

    /// All (start, end) token-index matches, like `find_matches` (every terminal on every walk).
    fn find(&self, text: &str, toks: &[Span], out: &mut Vec<(usize, usize)>) {
        out.clear();
        let text_of = |i: usize| &text[toks[i].0 as usize..toks[i].1 as usize];
        for start in 0..toks.len() {
            let Some(mut node) = self.child(0, text_of(start)) else { continue };
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
