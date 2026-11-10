//! `datatrove.utils.text.simplify_text`, the normalization MinHash shingles are built from.

use crate::pystr::{lower, nfd_without_mn, strip};
use crate::pyunicode::py;
use regex::Regex;

#[derive(Clone, Copy)]
pub struct NormConfig {
    pub lowercase: bool,
    pub norm_whitespace: bool,
    pub remove_punctuation: bool,
    pub norm_unicode_diacritics: bool,
    pub norm_numbers: bool,
    pub norm_weekdays: bool,
    pub norm_monthnames: bool,
}

impl Default for NormConfig {
    fn default() -> Self {
        Self {
            lowercase: true,
            norm_whitespace: true,
            remove_punctuation: true,
            norm_unicode_diacritics: true,
            norm_numbers: true,
            norm_weekdays: false,
            norm_monthnames: false,
        }
    }
}

const DECIMAL_SEPARATORS: [char; 7] = ['.', ',', '،', '٫', '⎖', '⎗', '⎘'];

pub struct Simplifier {
    cfg: NormConfig,
    weekdays: Regex,
    months: Regex,
    a: String,
    b: String,
    chars: Vec<char>,
}

impl Simplifier {
    pub fn new(cfg: NormConfig) -> Self {
        Self {
            cfg,
            weekdays: Regex::new("monday|tuesday|wednesday|thursday|friday|saturday|sunday").unwrap(),
            months: Regex::new(
                "january|february|march|april|may|june|july|august|september|october|november|december",
            )
            .unwrap(),
            a: String::new(),
            b: String::new(),
            chars: Vec::new(),
        }
    }

    /// Returns the simplified text; valid until the next call.
    pub fn simplify(&mut self, text: &str) -> &str {
        let c = self.cfg;
        let (a, b) = (&mut self.a, &mut self.b);
        a.clear();
        a.push_str(text);
        if c.lowercase {
            lower(a, b);
            std::mem::swap(a, b);
        }
        if c.norm_numbers {
            normalize_numbers(a, b);
            std::mem::swap(a, b);
        }
        if c.norm_weekdays {
            *a = self.weekdays.replace_all(a, "WEEKDAY").into_owned();
        }
        if c.norm_monthnames {
            *a = self.months.replace_all(a, "MONTH").into_owned();
        }
        if c.remove_punctuation {
            let p = &py().dt_punctuation;
            b.clear();
            b.extend(a.chars().map(|ch| if p.contains(ch) { ' ' } else { ch }));
            std::mem::swap(a, b);
        }
        if c.norm_whitespace {
            collapse_whitespace(strip(a), b);
            std::mem::swap(a, b);
        }
        if c.norm_unicode_diacritics {
            nfd_without_mn(a, &mut self.chars, b);
            std::mem::swap(a, b);
        }
        strip(a)
    }
}

/// `regex` `\p{Nd}+([.,،٫⎖⎗⎘]{1}\p{Nd}+)?` replaced by `"0"`.
fn normalize_numbers(s: &str, out: &mut String) {
    let nd = &py().regex_nd;
    out.clear();
    let mut it = s.chars().peekable();
    while let Some(ch) = it.next() {
        if !nd.contains(ch) {
            out.push(ch);
            continue;
        }
        while it.next_if(|&x| nd.contains(x)).is_some() {}
        let mut ahead = it.clone();
        if ahead.next().is_some_and(|x| DECIMAL_SEPARATORS.contains(&x)) && ahead.peek().is_some_and(|&x| nd.contains(x)) {
            it.next();
            while it.next_if(|&x| nd.contains(x)).is_some() {}
        }
        out.push('0');
    }
}

/// `re.sub(r"\s+", " ", s)`.
fn collapse_whitespace(s: &str, out: &mut String) {
    let ws = &py().re_s;
    out.clear();
    let mut in_ws = false;
    for ch in s.chars() {
        if ws.contains(ch) {
            if !in_ws {
                out.push(' ');
            }
            in_ws = true;
        } else {
            out.push(ch);
            in_ws = false;
        }
    }
}
