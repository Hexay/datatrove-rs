//! pyvi's syllable CRF (`ViTokenizer.sent2features` + crfsuite's Viterbi), bit-exact: state scores add the
//! same doubles in the same attribute order as crfsuite's `state_score`, and ties keep the first label.

use crate::pystr::lower;
use rustc_hash::{FxHashMap, FxHashSet};
use serde::Deserialize;
use spacy_tokenizer_rs::rules::CharSet;

#[derive(Deserialize)]
pub(super) struct Data {
    lower: Vec<(u32, u32)>,
    upper: Vec<(u32, u32)>,
    title: Vec<(u32, u32)>,
    digit: Vec<(u32, u32)>,
    trans: [[u64; 2]; 2],
    state: FxHashMap<String, [u64; 2]>,
    bi_grams: Vec<String>,
    tri_grams: Vec<String>,
}

pub(super) struct Crf {
    lower: CharSet,
    upper: CharSet,
    title: CharSet,
    pub digit: CharSet,
    trans: [[f64; 2]; 2],
    state: FxHashMap<String, [f64; 2]>,
    bi: FxHashSet<String>,
    tri: FxHashSet<String>,
}

/// Label index of pyvi's `I_W` (continue the previous word); 0 is `B_W`.
pub(super) const I_W: u8 = 1;

impl Crf {
    pub fn new(d: Data) -> Self {
        let f = |w: [u64; 2]| w.map(f64::from_bits);
        Self {
            lower: CharSet::new(&d.lower),
            upper: CharSet::new(&d.upper),
            title: CharSet::new(&d.title),
            digit: CharSet::new(&d.digit),
            trans: d.trans.map(f),
            state: d.state.into_iter().map(|(k, w)| (k, f(w))).collect(),
            bi: d.bi_grams.into_iter().collect(),
            tri: d.tri_grams.into_iter().collect(),
        }
    }

    /// `str.isupper()`.
    pub fn is_upper(&self, s: &str) -> bool {
        let mut cased = false;
        for c in s.chars() {
            if self.lower.contains(c) || self.title.contains(c) {
                return false;
            }
            cased |= self.upper.contains(c);
        }
        cased
    }

    /// `str.istitle()`.
    pub fn is_title(&self, s: &str) -> bool {
        let (mut cased, mut prev_cased) = (false, false);
        for c in s.chars() {
            if self.upper.contains(c) || self.title.contains(c) {
                if prev_cased {
                    return false;
                }
                prev_cased = true;
                cased = true;
            } else if self.lower.contains(c) {
                if !prev_cased {
                    return false;
                }
                prev_cased = true;
                cased = true;
            } else {
                prev_cased = false;
            }
        }
        cased
    }

    fn is_digit(&self, s: &str) -> bool {
        !s.is_empty() && s.chars().all(|c| self.digit.contains(c))
    }

    fn joined_in(&self, set: &FxHashSet<String>, words: &[&str], buf: &mut String) -> bool {
        let joined = words.join(" ");
        lower(&joined, buf);
        set.contains(buf.as_str())
    }

    /// `model.predict([sent2features(words)])[0]` as label indices.
    pub fn tag(&self, words: &[&str]) -> Vec<u8> {
        let n = words.len();
        let mut buf = String::new();
        let lw: Vec<String> = words
            .iter()
            .map(|w| {
                lower(w, &mut buf);
                buf.clone()
            })
            .collect();
        let up: Vec<bool> = words.iter().map(|w| self.is_upper(w)).collect();
        let ti: Vec<bool> = words.iter().map(|w| self.is_title(w)).collect();
        let bi: Vec<bool> = (0..n).map(|i| i > 0 && self.joined_in(&self.bi, &words[i - 1..=i], &mut buf)).collect();
        let tri: Vec<bool> = (0..n).map(|i| i > 1 && self.joined_in(&self.tri, &words[i - 2..=i], &mut buf)).collect();
        let mut scores = Vec::with_capacity(n);
        let mut key = String::new();
        for i in 0..n {
            let mut s = [0.0f64; 2];
            let mut add = |name: &str, on: bool| {
                if let (true, Some(w)) = (on, self.state.get(name)) {
                    s[0] += w[0];
                    s[1] += w[1];
                }
            };
            let mut add_str = |prefix: &str, value: &str, add: &mut dyn FnMut(&str, bool)| {
                key.clear();
                key.push_str(prefix);
                key.push_str(value);
                add(&key, true);
            };
            add("bias", true);
            add_str("word.lower():", &lw[i], &mut add);
            add("word.isupper()", up[i]);
            add("word.istitle()", ti[i]);
            add("word.isdigit()", self.is_digit(words[i]));
            if i > 0 {
                add_str("-1:word.lower():", &lw[i - 1], &mut add);
                add("-1:word.istitle()", ti[i - 1]);
                add("-1:word.isupper()", up[i - 1]);
                add("-1:word.bi_gram()", bi[i]);
                if i > 1 {
                    add("-2:word.tri_gram()", tri[i]);
                }
            }
            if i + 1 < n {
                add_str("+1:word.lower():", &lw[i + 1], &mut add);
                add("+1:word.istitle()", ti[i + 1]);
                add("+1:word.isupper()", up[i + 1]);
                add("+1:word.bi_gram()", bi[i + 1]);
                if i + 2 < n {
                    add("+2:word.tri_gram()", tri[i + 2]);
                }
            }
            scores.push(s);
        }
        self.viterbi(&scores)
    }

    /// crfsuite `crf1dc_viterbi`: strict `<` keeps the first best label on ties.
    fn viterbi(&self, state: &[[f64; 2]]) -> Vec<u8> {
        let n = state.len();
        if n == 0 {
            return Vec::new();
        }
        let mut back = vec![[0u8; 2]; n];
        let mut prev = state[0];
        for t in 1..n {
            let mut curr = [0.0; 2];
            for j in 0..2 {
                let (mut best, mut arg) = (-f64::MAX, 0u8);
                for (i, p) in prev.iter().enumerate() {
                    let score = p + self.trans[i][j];
                    if best < score {
                        best = score;
                        arg = i as u8;
                    }
                }
                back[t][j] = arg;
                curr[j] = best + state[t][j];
            }
            prev = curr;
        }
        let mut labels = vec![0u8; n];
        let mut best = -f64::MAX;
        for (i, &p) in prev.iter().enumerate() {
            if best < p {
                best = p;
                labels[n - 1] = i as u8;
            }
        }
        for t in (0..n - 1).rev() {
            labels[t] = back[t + 1][labels[t + 1] as usize];
        }
        labels
    }
}
