//! Python's Unicode tables, exported by `tools/export_pytext.py` (see its docstring for why).

use rustc_hash::{FxHashMap, FxHashSet};
use serde::Deserialize;
use spacy_tokenizer_rs::rules::CharSet;
use std::sync::OnceLock;

const JSON: &str = include_str!("../data/py_unicode.json");

#[derive(Deserialize)]
struct Tables {
    python_version: String,
    unidata_version: String,
    isspace: Vec<(u32, u32)>,
    isalpha: Vec<(u32, u32)>,
    punct: Vec<(u32, u32)>,
    mn: Vec<(u32, u32)>,
    re_d: Vec<(u32, u32)>,
    re_s: Vec<(u32, u32)>,
    re_w: Vec<(u32, u32)>,
    regex_nd: Vec<(u32, u32)>,
    linebreak: Vec<(u32, u32)>,
    sigma_ignorable: Vec<(u32, u32)>,
    sigma_cased: Vec<(u32, u32)>,
    ccc: Vec<(u32, u32, u8)>,
    lower: Vec<(u32, String)>,
    nfd: Vec<(u32, String)>,
    sentencizer_punct: Vec<String>,
    dt_punctuation: String,
    dt_punctuation_set: Vec<char>,
}

pub struct PyUnicode {
    /// datatrove `PUNCTUATION` (what `simplify_text` blanks out).
    pub dt_punctuation: CharSet,
    /// datatrove `PUNCTUATION_SET` (`PUNCTUATION` + `TERMINAL_PUNCTUATION`).
    pub dt_punctuation_set: CharSet,
    /// spaCy `Sentencizer.default_punct_chars`.
    pub sentencizer_punct: FxHashSet<Box<str>>,
    pub python_version: String,
    pub unidata_version: String,
    /// `str.isspace`, also what `str.strip()`/`str.split()` use.
    pub isspace: CharSet,
    pub isalpha: CharSet,
    /// `unicodedata.category(c).startswith("P")` — spaCy's `is_punct`.
    pub punct: CharSet,
    /// `unicodedata.category(c) == "Mn"`.
    pub mn: CharSet,
    /// `re` module `\d` / `\s` on str patterns.
    pub re_d: CharSet,
    pub re_s: CharSet,
    /// `re` module `\w` on str patterns (also decides `\b`).
    pub re_w: CharSet,
    /// `re_s`, `re_w`, `re_d` as inclusive code point ranges.
    pub re_ranges: [Vec<(u32, u32)>; 3],
    /// `regex` module `\p{Nd}` (its Unicode version can differ from `unicodedata`'s).
    pub regex_nd: CharSet,
    /// Characters `str.splitlines()` breaks on (`\r\n` is handled as a pair by the caller).
    pub linebreak: CharSet,
    pub sigma_ignorable: CharSet,
    pub sigma_cased: CharSet,
    ccc: Vec<(u32, u32, u8)>,
    lower: FxHashMap<u32, Box<str>>,
    nfd: FxHashMap<u32, Box<str>>,
}

impl PyUnicode {
    fn load() -> Self {
        let t: Tables = serde_json::from_str(JSON).expect("embedded py_unicode.json");
        let map = |v: Vec<(u32, String)>| v.into_iter().map(|(cp, s)| (cp, s.into_boxed_str())).collect();
        Self {
            dt_punctuation: char_set(t.dt_punctuation.chars()),
            dt_punctuation_set: char_set(t.dt_punctuation_set.into_iter()),
            sentencizer_punct: t.sentencizer_punct.into_iter().map(String::into_boxed_str).collect(),
            python_version: t.python_version,
            unidata_version: t.unidata_version,
            isspace: CharSet::new(&t.isspace),
            isalpha: CharSet::new(&t.isalpha),
            punct: CharSet::new(&t.punct),
            mn: CharSet::new(&t.mn),
            re_d: CharSet::new(&t.re_d),
            re_s: CharSet::new(&t.re_s),
            re_w: CharSet::new(&t.re_w),
            re_ranges: [t.re_s.clone(), t.re_w.clone(), t.re_d.clone()],
            regex_nd: CharSet::new(&t.regex_nd),
            linebreak: CharSet::new(&t.linebreak),
            sigma_ignorable: CharSet::new(&t.sigma_ignorable),
            sigma_cased: CharSet::new(&t.sigma_cased),
            ccc: t.ccc,
            lower: map(t.lower),
            nfd: map(t.nfd),
        }
    }

    /// Full lowercase mapping of one char, excluding U+03A3 (context-dependent, see `pystr::lower`).
    #[inline]
    pub fn lower_of(&self, c: char) -> Option<&str> {
        if c.is_ascii() {
            return None;
        }
        self.lower.get(&(c as u32)).map(|s| &**s)
    }

    #[inline]
    pub fn nfd_of(&self, c: char) -> Option<&str> {
        if c.is_ascii() {
            return None;
        }
        self.nfd.get(&(c as u32)).map(|s| &**s)
    }

    /// Canonical combining class.
    #[inline]
    pub fn ccc(&self, c: char) -> u8 {
        let cp = c as u32;
        if cp < 0x300 {
            return 0;
        }
        match self.ccc.binary_search_by(|&(lo, _, _)| lo.cmp(&cp)) {
            Ok(i) => self.ccc[i].2,
            Err(0) => 0,
            Err(i) if cp <= self.ccc[i - 1].1 => self.ccc[i - 1].2,
            Err(_) => 0,
        }
    }
}

fn char_set(chars: impl Iterator<Item = char>) -> CharSet {
    let mut cps: Vec<u32> = chars.map(|c| c as u32).collect();
    cps.sort_unstable();
    cps.dedup();
    let mut ranges: Vec<(u32, u32)> = Vec::new();
    for cp in cps {
        match ranges.last_mut() {
            Some(r) if r.1 + 1 == cp => r.1 = cp,
            _ => ranges.push((cp, cp)),
        }
    }
    CharSet::new(&ranges)
}

pub fn py() -> &'static PyUnicode {
    static T: OnceLock<PyUnicode> = OnceLock::new();
    T.get_or_init(PyUnicode::load)
}
