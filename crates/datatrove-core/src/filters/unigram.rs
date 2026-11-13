//! `UnigramLogProbFilter.get_logprob`. Logs come precomputed by numpy (`python/datatrove_rs/unigram.py`); the
//! sum runs left to right from 0.0 and is divided once, as Python's `sum` of `np.float64`s then `/ len` does.

use crate::pystr::lower;
use crate::words::Words;
use rustc_hash::FxHashMap;

pub struct Unigram {
    words: Words,
    logs: FxHashMap<Box<str>, f64>,
    missing: f64,
    lower: String,
}

impl Unigram {
    /// `logs[i]` = `np.log(unigram_frequencies[words[i]])`; `missing` = `np.log(1e-9)`.
    pub fn new(words: Vec<String>, logs: Vec<f64>, missing: f64, tokenizer: Words) -> Self {
        let logs = words.into_iter().map(String::into_boxed_str).zip(logs).collect();
        Self { words: tokenizer, logs, missing, lower: String::new() }
    }

    /// Mean log-probability of the doc's words; None when it has none (datatrove returns 0).
    pub fn mean_logprob(&mut self, text: &str) -> Option<f64> {
        let mut tokens = Vec::new();
        self.words.words(text, &mut tokens);
        if tokens.is_empty() {
            return None;
        }
        let mut sum = 0.0;
        for w in &tokens {
            lower(w, &mut self.lower);
            sum += self.logs.get(self.lower.as_str()).copied().unwrap_or(self.missing);
        }
        Some(sum / tokens.len() as f64)
    }
}
