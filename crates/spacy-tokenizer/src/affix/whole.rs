use super::{anchored, fancy, rx};
use crate::pyalt::{has_lookaround, strip_lookarounds};
use crate::rules::PyClasses;
use regex::Regex;

/// `re.match` for token_match/url_match: `regex` for lookaround-free patterns; otherwise a
/// relaxed `regex` (lookarounds deleted, so it accepts a superset) as prefilter, fancy-regex to confirm.
pub struct WholeMatcher {
    fast: Regex,
    confirm: Option<fancy_regex::Regex>,
}

impl WholeMatcher {
    pub fn new(py: &Option<String>, cls: &PyClasses) -> Result<Option<Self>, String> {
        let Some(py) = py else { return Ok(None) };
        if !has_lookaround(py) {
            return Ok(Some(Self { fast: rx(&anchored(py, cls)?)?, confirm: None }));
        }
        let fast = rx(&anchored(&strip_lookarounds(py), cls)?)?;
        Ok(Some(Self { fast, confirm: Some(fancy(&anchored(py, cls)?)?) }))
    }

    #[inline]
    pub fn is_match(m: &Option<Self>, s: &str) -> bool {
        let Some(m) = m else { return false };
        m.fast.is_match(s) && m.confirm.as_ref().map_or(true, |c| c.is_match(s).expect("backtrack limit"))
    }
}
