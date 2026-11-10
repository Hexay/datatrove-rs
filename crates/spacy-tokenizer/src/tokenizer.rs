//! Port of `spacy/tokenizer.pyx` (v3.8): `_tokenize_affixes`, `_split_affixes`, `_attach_tokens`,
//! `_apply_special_cases`. Tokens are (start, end) byte spans; every chunk is covered exactly, so
//! tokens are pushed as lengths from a cursor (spaCy's `idx` bookkeeping reduces to this).

use crate::affix::{self, Affixes, Backend};
use crate::rules::{CharSet, PyClasses, RulesJson};
use crate::specials::{self, hash_str, LensTable, PhraseTrie, Span, Specials};

pub struct Tokenizer {
    aff: Box<dyn Affixes>,
    specials: Specials,
    matcher: PhraseTrie,
    space: CharSet,
    pub spacy_version: String,
}

/// Per-caller mutable state: output, buffers and spaCy's chunk cache.
pub struct State {
    pub tokens: Vec<Span>,
    cache: LensTable,
    cache_max: usize,
    prefixes: Vec<u32>,
    suffixes: Vec<u32>,
    infix: Vec<(usize, usize)>,
    special: specials::Scratch,
}

impl State {
    /// `cache_max` = spaCy's `max_cache_size` (10_000); 0 emulates `memory_zone` (never caches).
    pub fn new(cache_max: usize) -> Self {
        Self {
            tokens: Vec::new(),
            cache: LensTable::default(),
            cache_max,
            prefixes: Vec::new(),
            suffixes: Vec::new(),
            infix: Vec::new(),
            special: Default::default(),
        }
    }
}

struct Chunk<'a> {
    text: &'a str,
    base: u32,
    cursor: u32,
    has_special: bool,
}

impl Tokenizer {
    pub fn from_json(json: &str, backend: Backend) -> Result<Self, String> {
        let r = RulesJson::from_str(json)?;
        let cls = PyClasses::new(&r);
        let mut tok = Self {
            aff: affix::build(&r, &cls, backend)?,
            specials: Specials::new(&r.rules)?,
            matcher: PhraseTrie::new(),
            space: CharSet::new(&r.py_isspace),
            spacy_version: r.spacy_version.clone(),
        };
        let mut st = State::new(0);
        let mut infix = Vec::new();
        for (chunk, _) in &r.rules {
            tok.aff.infixes(chunk, &mut infix);
            let needs_matcher = !r.faster_heuristics
                || tok.aff.prefix_len(chunk) != 0
                || !infix.is_empty()
                || tok.aff.suffix_len(chunk) != 0
                || chunk.contains(' ');
            if needs_matcher {
                tok.tokenize_affixes(chunk, false, &mut st);
                let parts: Vec<&str> = st.tokens.iter().map(|&(a, b)| &chunk[a as usize..b as usize]).collect();
                tok.matcher.add(&parts);
            }
        }
        Ok(tok)
    }

    pub fn affixes(&self) -> &dyn Affixes {
        &*self.aff
    }

    pub fn matcher_patterns(&self) -> usize {
        self.matcher.len()
    }

    /// `Tokenizer.__call__`: tokens land in `st.tokens`.
    pub fn tokenize(&self, text: &str, st: &mut State) {
        self.tokenize_affixes(text, true, st);
        let State { tokens, special, .. } = st;
        self.matcher.apply(&self.specials, text, tokens, special);
    }

    /// Diagnostics only: skips the PhraseMatcher pass, to measure how often it changes output.
    pub fn tokenize_without_matcher(&self, text: &str, st: &mut State) {
        self.tokenize_affixes(text, true, st);
    }

    fn tokenize_affixes(&self, text: &str, with_special: bool, st: &mut State) {
        st.tokens.clear();
        let bytes = text.as_bytes();
        if bytes.is_empty() {
            return;
        }
        let mut has_special = false;
        let mut in_ws = self.space_at(text, 0).0;
        let (mut start, mut i) = (0, 0);
        while i < bytes.len() {
            let (ws, len) = self.space_at(text, i);
            if ws != in_ws {
                if start < i {
                    self.chunk(text, start, i, with_special, &mut has_special, st);
                }
                // A single ' ' after a token is absorbed as that token's trailing space.
                start = if bytes[i] == b' ' { i + 1 } else { i };
                in_ws = !in_ws;
            }
            i += len;
        }
        if start < text.len() {
            self.chunk(text, start, text.len(), with_special, &mut has_special, st);
        }
    }

    /// (isspace, UTF-8 length) of the char starting at byte `i`; ASCII skips UTF-8 decoding.
    #[inline(always)]
    fn space_at(&self, text: &str, i: usize) -> (bool, usize) {
        let b = text.as_bytes()[i];
        if b < 0x80 {
            return (self.space.contains(b as char), 1);
        }
        let c = text[i..].chars().next().unwrap();
        (self.space.contains(c), c.len_utf8())
    }

    fn chunk(&self, text: &str, a: usize, b: usize, with_special: bool, has_special: &mut bool, st: &mut State) {
        let span = &text[a..b];
        let mut c = Chunk { text: span, base: a as u32, cursor: 0, has_special: *has_special };
        let orig = st.tokens.len();
        let h = hash_str(span);
        if !self.try_specials_and_cache(&mut c, h, span, with_special, st) {
            let (core_a, core_b) = self.split_affixes(span, with_special, st);
            self.attach_tokens(&mut c, core_a, core_b, with_special, st);
            if !c.has_special && st.cache.len() < st.cache_max {
                let lens = st.tokens[orig..].iter().map(|&(x, y)| y - x).collect();
                st.cache.insert_new(h, span, lens);
            }
        }
        *has_special = c.has_special;
    }

    #[inline]
    fn push(c: &mut Chunk, st: &mut State, len: u32) {
        Self::push_to(c, &mut st.tokens, len);
    }

    #[inline]
    fn push_to(c: &mut Chunk, tokens: &mut Vec<Span>, len: u32) {
        let s = c.base + c.cursor;
        tokens.push((s, s + len));
        c.cursor += len;
    }

    /// spaCy checks specials before the cache; probing the cache first is equivalent because only
    /// non-special strings are ever cached, and it skips the specials probe on the common hit.
    fn try_specials_and_cache(&self, c: &mut Chunk, h: u64, s: &str, with_special: bool, st: &mut State) -> bool {
        let State { cache, tokens, .. } = &mut *st;
        if let Some(lens) = cache.get(h, s) {
            lens.iter().for_each(|&l| Self::push_to(c, tokens, l));
            return true;
        }
        if with_special {
            if let Some(lens) = self.specials.get_hashed(h, s) {
                lens.iter().for_each(|&l| Self::push(c, st, l));
                c.has_special = true;
                return true;
            }
        }
        false
    }

    /// Returns the core's byte range within `s`; prefix/suffix lengths go to `st`.
    fn split_affixes(&self, s: &str, with_special: bool, st: &mut State) -> (usize, usize) {
        st.prefixes.clear();
        st.suffixes.clear();
        let (mut a, mut b) = (0, s.len());
        let mut last_size = usize::MAX;
        while a < b && b - a != last_size {
            let cur = &s[a..b];
            if self.aff.token_match(cur) || (with_special && self.specials.contains(cur)) {
                break;
            }
            last_size = b - a;
            let pre = self.aff.prefix_len(cur);
            if pre != 0 && pre < cur.len() && with_special && self.specials.contains(&cur[pre..]) {
                st.prefixes.push(pre as u32);
                a += pre;
                break;
            }
            let suf = self.aff.suffix_len(&cur[pre..]);
            if suf != 0 && suf < cur.len() && with_special && self.specials.contains(&cur[..cur.len() - suf]) {
                st.suffixes.push(suf as u32);
                b -= suf;
                break;
            }
            if pre != 0 {
                st.prefixes.push(pre as u32);
                a += pre;
            }
            if suf != 0 {
                st.suffixes.push(suf as u32);
                b -= suf;
            }
        }
        (a, b)
    }

    fn attach_tokens(&self, c: &mut Chunk, a: usize, b: usize, with_special: bool, st: &mut State) {
        let State { prefixes, tokens, .. } = &mut *st;
        prefixes.iter().for_each(|&l| Self::push_to(c, tokens, l));
        let core = &c.text[a..b];
        if !core.is_empty() && !self.try_specials_and_cache(c, hash_str(core), core, with_special, st) {
            if self.aff.token_match(core) || self.aff.url_match(core) {
                Self::push(c, st, core.len() as u32);
            } else {
                let mut infix = std::mem::take(&mut st.infix);
                self.aff.infixes(core, &mut infix);
                let mut start = 0;
                for &(is, ie) in &infix {
                    // spaCy compares against the start of the string, not the running `start`.
                    if is == 0 {
                        continue;
                    }
                    if is != start {
                        Self::push(c, st, (is - start) as u32);
                    }
                    if is != ie {
                        Self::push(c, st, (ie - is) as u32);
                    }
                    start = ie;
                }
                if start < core.len() {
                    Self::push(c, st, (core.len() - start) as u32);
                }
                st.infix = infix;
            }
        }
        let State { suffixes, tokens, .. } = st;
        suffixes.iter().rev().for_each(|&l| Self::push_to(c, tokens, l));
    }
}
