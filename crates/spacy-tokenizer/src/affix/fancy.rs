use super::{anchored, fancy, next_char_boundary, Affixes};
use crate::pyre::translate;
use crate::rules::{PyClasses, RulesJson};
use fancy_regex::Regex;

pub struct FancyAffixes {
    prefix: Option<Regex>,
    suffix: Option<Regex>,
    infix: Option<Regex>,
    token_match: Option<Regex>,
    url_match: Option<Regex>,
}

fn compile(py: &Option<String>, cls: &PyClasses, anchor: bool) -> Result<Option<Regex>, String> {
    py.as_deref().map(|p| fancy(&if anchor { anchored(p, cls)? } else { translate(p, cls)? })).transpose()
}

impl FancyAffixes {
    pub fn new(r: &RulesJson, cls: &PyClasses) -> Result<Self, String> {
        Ok(Self {
            prefix: compile(&r.prefix, cls, false)?,
            suffix: compile(&r.suffix, cls, false)?,
            infix: compile(&r.infix, cls, false)?,
            token_match: compile(&r.token_match, cls, true)?,
            url_match: compile(&r.url_match, cls, true)?,
        })
    }
}

fn search_len(re: &Option<Regex>, s: &str) -> usize {
    re.as_ref().and_then(|re| re.find(s).expect("backtrack limit")).map_or(0, |m| m.end() - m.start())
}

impl Affixes for FancyAffixes {
    fn prefix_len(&self, s: &str) -> usize {
        search_len(&self.prefix, s)
    }

    fn suffix_len(&self, s: &str) -> usize {
        search_len(&self.suffix, s)
    }

    fn infixes(&self, s: &str, out: &mut Vec<(usize, usize)>) {
        out.clear();
        let Some(re) = &self.infix else { return };
        let mut pos = 0;
        while pos <= s.len() {
            let Some(m) = re.find_from_pos(s, pos).expect("backtrack limit") else { break };
            out.push((m.start(), m.end()));
            // TODO: Python may still take a non-empty match at an empty match's position (see SplitAffixes::infixes).
            pos = if m.end() == m.start() { next_char_boundary(s, m.end()) } else { m.end() };
        }
    }

    fn token_match(&self, s: &str) -> bool {
        self.token_match.as_ref().is_some_and(|re| re.is_match(s).expect("backtrack limit"))
    }

    fn url_match(&self, s: &str) -> bool {
        self.url_match.as_ref().is_some_and(|re| re.is_match(s).expect("backtrack limit"))
    }
}
