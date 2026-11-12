//! `jieba.finalseg.cut`: Viterbi over B/M/E/S states for runs of unknown Han chars.

use crate::Span;
use rustc_hash::{FxHashMap, FxHashSet};

const MIN_FLOAT: f64 = -3.14e100;
const NAMES: [char; 4] = ['B', 'M', 'E', 'S'];
const B: usize = 0;
const M: usize = 1;
const E: usize = 2;
const S: usize = 3;
/// `PrevStatus`, in its string order: B <- "ES", M <- "MB", E <- "BM", S <- "SE".
const PREV: [[usize; 2]; 4] = [[E, S], [M, B], [B, M], [S, E]];

/// `start_P`, `trans_P`, `emit_P` and `Force_Split_Words`.
pub struct Hmm {
    start: [f64; 4],
    trans: [[f64; 4]; 4],
    emit: [FxHashMap<char, f64>; 4],
    force_split: FxHashSet<Box<str>>,
}

fn state(name: &str) -> Result<usize, String> {
    NAMES.iter().position(|&c| name.len() == 1 && name.starts_with(c)).ok_or_else(|| format!("bad state {name:?}"))
}

impl Hmm {
    /// Tables keyed by state name ("B", "M", "E", "S"); each state needs a start probability and a trans and emit
    /// table (as finalseg indexes them), missing transitions and emissions are `MIN_FLOAT`.
    pub fn new(
        start: Vec<(String, f64)>,
        trans: Vec<(String, Vec<(String, f64)>)>,
        emit: Vec<(String, Vec<(String, f64)>)>,
        force_split: Vec<String>,
    ) -> Result<Self, String> {
        let mut st = [None; 4];
        for (k, p) in start {
            st[state(&k)?] = Some(p);
        }
        let mut tr = [[MIN_FLOAT; 4]; 4];
        let mut seen = [false; 4];
        for (k, row) in trans {
            let from = state(&k)?;
            seen[from] = true;
            for (k2, p) in row {
                if let Ok(to) = state(&k2) {
                    tr[from][to] = p;
                }
            }
        }
        let mut em: [FxHashMap<char, f64>; 4] = Default::default();
        let mut seen_emit = [false; 4];
        for (k, row) in emit {
            let s = state(&k)?;
            seen_emit[s] = true;
            em[s] = row.into_iter().filter_map(|(c, p)| single_char(&c).map(|c| (c, p))).collect();
        }
        if st.iter().any(Option::is_none) || seen.contains(&false) || seen_emit.contains(&false) {
            return Err("HMM tables miss a state".into());
        }
        Ok(Self {
            start: st.map(Option::unwrap),
            trans: tr,
            emit: em,
            force_split: force_split.into_iter().map(String::into_boxed_str).collect(),
        })
    }

    /// The tables as `tools/cjk_ref.py hmm` exports them (floats as IEEE-754 bit patterns).
    pub fn from_json(json: &str) -> Result<Self, String> {
        use serde_json::Value;
        let v: Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
        let f = |x: &Value| f64::from_bits(x.as_u64().unwrap_or(0));
        let row = |x: &Value| -> Vec<(String, f64)> {
            x.as_object().map(|o| o.iter().map(|(k, p)| (k.clone(), f(p))).collect()).unwrap_or_default()
        };
        let table = |x: &Value| -> Vec<(String, Vec<(String, f64)>)> {
            x.as_object().map(|o| o.iter().map(|(k, r)| (k.clone(), row(r))).collect()).unwrap_or_default()
        };
        let force = v["force_split"].as_array().map(|a| a.iter().filter_map(|s| s.as_str().map(Into::into)).collect());
        Self::new(row(&v["start"]), table(&v["trans"]), table(&v["emit"]), force.unwrap_or_default())
    }

    fn emit(&self, s: usize, c: char) -> f64 {
        self.emit[s].get(&c).copied().unwrap_or(MIN_FLOAT)
    }

    /// `viterbi(obs, 'BMES', ...)`: the best state path. Python's max over (prob, state) tuples breaks ties by
    /// the larger state letter.
    fn viterbi(&self, obs: &[char]) -> Vec<usize> {
        let better = |p: f64, s: usize, best: Option<(f64, usize)>| {
            best.is_none_or(|(bp, bs)| p > bp || (p == bp && NAMES[s] > NAMES[bs]))
        };
        let mut v: [f64; 4] = std::array::from_fn(|y| self.start[y] + self.emit(y, obs[0]));
        let mut back: Vec<[usize; 4]> = Vec::with_capacity(obs.len());
        for &c in &obs[1..] {
            let mut nv = [0f64; 4];
            let mut bp = [0usize; 4];
            for y in 0..4 {
                let em = self.emit(y, c);
                let mut best = None;
                for &y0 in &PREV[y] {
                    let p = v[y0] + self.trans[y0][y] + em;
                    if better(p, y0, best) {
                        best = Some((p, y0));
                    }
                }
                (nv[y], bp[y]) = best.unwrap();
            }
            v = nv;
            back.push(bp);
        }
        let mut s = if better(v[S], S, Some((v[E], E))) { S } else { E };
        let mut path = vec![s; obs.len()];
        for (t, bp) in back.iter().enumerate().rev() {
            s = bp[s];
            path[t] = s;
        }
        path
    }

    /// `finalseg.cut(text)`, pushing byte spans offset by `base`.
    pub fn cut(&self, text: &str, base: u32, out: &mut Vec<Span>) {
        let is_han = |c: char| ('\u{4E00}'..='\u{9FD5}').contains(&c);
        let mut i = 0;
        while i < text.len() {
            let han = is_han(text[i..].chars().next().unwrap());
            let end = text[i..].find(|c| is_han(c) != han).map_or(text.len(), |k| i + k);
            if han {
                self.cut_han(&text[i..end], base + i as u32, out);
            } else {
                cut_skip(&text[i..end], base + i as u32, out);
            }
            i = end;
        }
    }

    /// `__cut` on a Han block, splitting `Force_Split_Words` into chars.
    fn cut_han(&self, s: &str, base: u32, out: &mut Vec<Span>) {
        let pos: Vec<usize> = s.char_indices().map(|(i, _)| i).chain(std::iter::once(s.len())).collect();
        let chars: Vec<char> = s.chars().collect();
        let mut emit = |x: usize, y: usize| {
            let w = &s[pos[x]..pos[y]];
            if self.force_split.contains(w) {
                out.extend((x..y).map(|k| (base + pos[k] as u32, base + pos[k + 1] as u32)));
            } else {
                out.push((base + pos[x] as u32, base + pos[y] as u32));
            }
        };
        let (mut begin, mut next) = (0, 0);
        for (i, st) in self.viterbi(&chars).into_iter().enumerate() {
            match st {
                B => begin = i,
                E => {
                    emit(begin, i + 1);
                    next = i + 1;
                }
                S => {
                    emit(i, i + 1);
                    next = i + 1;
                }
                _ => {}
            }
        }
        if next < chars.len() {
            emit(next, chars.len());
        }
    }
}

fn single_char(s: &str) -> Option<char> {
    let mut it = s.chars();
    it.next().filter(|_| it.next().is_none())
}

/// `re_skip.split(blk)` with `([a-zA-Z0-9]+(?:\.\d+)?%?)`, non-empty pieces. Only ASCII reaches here.
fn cut_skip(s: &str, base: u32, out: &mut Vec<Span>) {
    let b = s.as_bytes();
    let mut push = |x: usize, y: usize| out.push((base + x as u32, base + y as u32));
    let (mut gap, mut i) = (0, 0);
    while i < b.len() {
        if !b[i].is_ascii_alphanumeric() {
            i += s[i..].chars().next().unwrap().len_utf8();
            continue;
        }
        let start = i;
        while i < b.len() && b[i].is_ascii_alphanumeric() {
            i += 1;
        }
        if i + 1 < b.len() && b[i] == b'.' && b[i + 1].is_ascii_digit() {
            i += 2;
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
        }
        if i < b.len() && b[i] == b'%' {
            i += 1;
        }
        if start > gap {
            push(gap, start);
        }
        push(start, i);
        gap = i;
    }
    if gap < b.len() {
        push(gap, b.len());
    }
}
