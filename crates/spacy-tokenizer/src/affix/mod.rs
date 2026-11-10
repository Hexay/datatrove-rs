//! Regex backends for spaCy's five tokenizer callables. All offsets are byte offsets into `s`.

mod edges;
mod fancy;
mod split;
mod whole;

pub use fancy::FancyAffixes;
pub use split::SplitAffixes;

use crate::pyre::translate;
use crate::rules::{PyClasses, RulesJson};

pub trait Affixes: Send + Sync {
    /// `prefix_search(s)` match length (Python returns `end - start`).
    fn prefix_len(&self, s: &str) -> usize;
    fn suffix_len(&self, s: &str) -> usize;
    /// `infix_finditer(s)` spans, in order.
    fn infixes(&self, s: &str, out: &mut Vec<(usize, usize)>);
    fn token_match(&self, s: &str) -> bool;
    fn url_match(&self, s: &str) -> bool;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    /// fancy-regex on the verbatim translated patterns.
    Fancy,
    /// `regex` crate, with fixed-width lookarounds peeled off and checked by hand.
    Split,
}

pub fn build(r: &RulesJson, cls: &PyClasses, backend: Backend) -> Result<Box<dyn Affixes>, String> {
    Ok(match backend {
        Backend::Fancy => Box::new(FancyAffixes::new(r, cls)?),
        Backend::Split => Box::new(SplitAffixes::new(r, cls)?),
    })
}

pub(crate) fn rx(pattern: &str) -> Result<regex::Regex, String> {
    regex::RegexBuilder::new(pattern)
        .size_limit(256 << 20)
        .dfa_size_limit(64 << 20)
        .build()
        .map_err(|e| e.to_string())
}

pub(crate) fn fancy(pattern: &str) -> Result<fancy_regex::Regex, String> {
    fancy_regex::RegexBuilder::new(pattern)
        .delegate_size_limit(256 << 20)
        .delegate_dfa_size_limit(64 << 20)
        .backtrack_limit(10_000_000)
        .build()
        .map_err(|e| e.to_string())
}

/// `re.match` semantics for token_match/url_match: anchored at start, `$` inside the pattern.
pub(crate) fn anchored(py: &str, cls: &PyClasses) -> Result<String, String> {
    Ok(format!("^(?:{})", translate(py, cls)?))
}

#[inline]
pub(crate) fn next_char_boundary(s: &str, i: usize) -> usize {
    i + s[i..].chars().next().map_or(1, char::len_utf8)
}
