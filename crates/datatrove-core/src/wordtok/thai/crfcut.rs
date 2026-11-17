//! pythainlp 5.4.0 `tokenize.crfcut.segment`: `_extract_features` over `word_tokenize(text)` (newmm, whitespace
//! kept), crfsuite tagging with the model file pythainlp ships, then its label fixes. The model, `_ENDERS` and
//! `_STARTERS` come from the installed pythainlp.

use super::super::crfsuite::Model;
use crate::pystr::strip;
use rustc_hash::FxHashSet;
use std::fmt::Write;

const WINDOW: usize = 2;
const MAX_N_GRAM: usize = 3;
const PAD: &str = "xxpad";

pub(super) struct CrfCut {
    model: Model,
    enders: FxHashSet<String>,
    starters: FxHashSet<String>,
    end: u32,
    inside: u32,
}

impl CrfCut {
    pub fn new(model: &[u8], enders: Vec<String>, starters: Vec<String>) -> Result<Self, String> {
        let model = Model::parse(model)?;
        let label = |name| model.label(name).ok_or_else(|| format!("crfcut model has no label {name:?}"));
        let (end, inside) = (label("E")?, label("I")?);
        let (enders, starters) = (enders.into_iter().collect(), starters.into_iter().collect());
        Ok(Self { model, enders, starters, end, inside })
    }

    /// `segment(text)` for `tokens`, the byte spans of `word_tokenize(text)` (contiguous, covering `text`).
    pub fn segment<'a>(&self, text: &'a str, tokens: &[(usize, usize)]) -> Vec<&'a str> {
        let toks: Vec<&str> = tokens.iter().map(|&(a, b)| &text[a..b]).collect();
        let mut labs = self.tag(&toks);
        let Some(last) = labs.last_mut() else { return Vec::new() };
        *last = self.end;
        for (i, tok) in toks.iter().enumerate() {
            let s = strip(tok);
            if s.ends_with(['!', '.', '?']) {
                labs[i] = self.end;
            } else if (i == 0 || labs[i - 1] == self.end) && s.is_empty() {
                labs[i] = self.inside;
            }
        }
        let mut out = Vec::new();
        let mut start = 0;
        for (&(_, b), &lab) in tokens.iter().zip(&labs) {
            if lab == self.end && b > start {
                out.push(&text[start..b]);
                start = b;
            }
        }
        out
    }

    /// `_tagger.tag(_extract_features(toks))`: each item's attributes in `_extract_features` order.
    fn tag(&self, toks: &[&str]) -> Vec<u32> {
        let pad = [PAD; WINDOW];
        let doc: Vec<&str> = pad.iter().chain(toks).chain(&pad).copied().collect();
        let flags = |set: &FxHashSet<String>, hit: &'static str| -> Vec<&'static str> {
            doc.iter().map(|&t| if set.contains(t) { hit } else { "normal" }).collect()
        };
        let (ender, starter) = (flags(&self.enders, "ender"), flags(&self.starters, "starter"));
        let l = self.model.labels.len();
        let mut state = vec![0.0; toks.len() * l];
        let mut key = String::new();
        for (item, scores) in state.chunks_mut(l).enumerate() {
            let i = item + WINDOW;
            self.model.add_state("bias", scores);
            for n in 1..=MAX_N_GRAM.min(1 + 2 * WINDOW) {
                for j in i - WINDOW..i + WINDOW + 2 - n {
                    let a = j as isize - i as isize;
                    for (name, seq) in [("word", &doc), ("ender", &ender), ("starter", &starter)] {
                        key.clear();
                        let _ = write!(key, "{name}_{n}_{a}_{}=", a + n as isize);
                        for (k, t) in seq[j..j + n].iter().enumerate() {
                            if k > 0 {
                                key.push('|');
                            }
                            key.push_str(t);
                        }
                        self.model.add_state(&key, scores);
                    }
                }
            }
        }
        self.model.viterbi(&state)
    }
}
