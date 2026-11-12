//! `regex`-crate backend. Each top-level alternative with a fixed-width lookbehind and/or a
//! single-token lookahead becomes a `LookAlt` (see `look.rs`), reproducing Python's backtracking order:
//! alternatives are tried in pattern order at the leftmost position (prefix/infix), and for
//! `$`-anchored suffixes the result is simply the leftmost start over all alternatives.

use super::edges::edge_set;
use super::look::LookAlt;
use super::whole::WholeMatcher;
use super::{next_char_boundary, rx, Affixes};
use crate::pyalt::{has_lookaround, split_top, strip_lookarounds, PyAlt};
use crate::pyre::translate;
use crate::rules::{CharSet, PyClasses, RulesJson};
use regex::Regex;

const MAX_RUNS: usize = 16;

/// `edge`: chars a match can start with (prefix/infix runs) or end with (suffix runs); see `admits`.
enum Run {
    Plain { find: Regex, at_start: Regex, edge: Option<CharSet> },
    Look(LookAlt),
}

impl Run {
    #[inline]
    fn edge(&self) -> &Option<CharSet> {
        match self {
            Run::Plain { edge, .. } => edge,
            Run::Look(l) => &l.edge,
        }
    }

    /// Leftmost (start, len) at or after `pos`, Python `finditer` style (see `SplitAffixes::infixes`).
    fn find_from(&self, s: &str, pos: usize, must_advance: bool) -> Option<(usize, usize)> {
        match self {
            Run::Plain { find, .. } => find.find_at(s, pos).map(|m| (m.start(), m.end() - m.start())),
            Run::Look(l) => l.find_from(s, pos, must_advance),
        }
    }
}

fn plain_regex(body: &str, suffix: bool) -> Result<Regex, String> {
    rx(&if suffix { format!("(?:{body})$") } else { format!("(?:{body})") })
}

/// Group consecutive lookaround-free alternatives into one `regex`, preserving priority order.
fn build_runs(py: &str, cls: &PyClasses, suffix: bool) -> Result<Vec<Run>, String> {
    let alts: Vec<PyAlt> = split_top(py).into_iter().map(PyAlt::parse).collect();
    let mut runs = Vec::new();
    let mut plain: Vec<String> = Vec::new();
    let flush = |plain: &mut Vec<String>, runs: &mut Vec<Run>| -> Result<(), String> {
        if !plain.is_empty() {
            let body = plain.join("|");
            let find = plain_regex(&body, suffix)?;
            // Plain runs can't emulate `finditer`'s retry after an empty match; none of spaCy's do.
            if !suffix && find.is_match("") {
                return Err(format!("plain alternative matching empty: {body}"));
            }
            runs.push(Run::Plain { find, at_start: rx(&format!("^(?:{body})"))?, edge: edge_set(&body, !suffix) });
            plain.clear();
        }
        Ok(())
    };
    for alt in &alts {
        if has_lookaround(&alt.core) {
            return Err(format!("nested lookaround in {}", alt.core));
        }
        if alt.is_plain() {
            plain.push(translate(&alt.core, cls)?);
        } else {
            flush(&mut plain, &mut runs)?;
            runs.push(Run::Look(LookAlt::new(alt, &alt.core, cls, suffix)?));
        }
    }
    flush(&mut plain, &mut runs)?;
    if runs.len() > MAX_RUNS {
        return Err(format!("{} runs > {MAX_RUNS}", runs.len()));
    }
    Ok(runs)
}

/// For `$`-anchored suffixes only the leftmost start matters, so lookbehind alternatives that
/// share a lookbehind are merged into one core regex. Whole-string alternatives `^y$` (el) become one
/// plain run: they can only match at 0, which is the leftmost start anyway.
fn build_suffix(py: &str, cls: &PyClasses) -> Result<Vec<Run>, String> {
    let alts: Vec<PyAlt> = split_top(py).into_iter().map(PyAlt::parse).collect();
    if alts.iter().any(|a| !a.end_anchor || !a.lookahead.is_empty() || a.start_anchor && a.lookbehind.is_some()) {
        return Err("suffix alternative not of form (?<=x)y$ or ^y$".into());
    }
    let (whole, alts): (Vec<PyAlt>, Vec<PyAlt>) = alts.into_iter().partition(|a| a.start_anchor);
    let mut runs = Vec::new();
    if !whole.is_empty() {
        let body = whole.iter().map(|a| translate(&a.core, cls)).collect::<Result<Vec<_>, _>>()?.join("|");
        let find = rx(&format!("^(?:{body})$"))?;
        runs.push(Run::Plain { at_start: find.clone(), find, edge: edge_set(&body, false) });
    }
    let mut groups: Vec<(Option<String>, Vec<String>)> = Vec::new();
    for a in &alts {
        match groups.iter_mut().find(|(lb, _)| *lb == a.lookbehind) {
            Some((_, cores)) => cores.push(a.core.clone()),
            None => groups.push((a.lookbehind.clone(), vec![a.core.clone()])),
        }
    }
    for (lb, cores) in groups {
        let core = format!("(?:{})", cores.join("|"));
        let alt =
            PyAlt { start_anchor: false, end_anchor: true, lookbehind: lb, lookahead: vec![], core: core.clone() };
        runs.push(Run::Look(LookAlt::new(&alt, &core, cls, true)?));
    }
    Ok(runs)
}

/// spaCy joins prefixes as `^a|^b|...`, but a punctuation entry containing `|` leaves an alternative
/// without `^` (id, ms: `<a([^>]+)>`), which `prefix_search` (a `re.search`) then finds anywhere in the string.
/// Returns those alternatives as one regex, for the search past position 0.
fn unanchored_prefixes(py: &str, cls: &PyClasses) -> Result<Option<Regex>, String> {
    let loose: Vec<&str> = split_top(py).into_iter().filter(|a| !a.starts_with('^')).collect();
    if loose.is_empty() {
        return Ok(None);
    }
    if loose.iter().any(|a| has_lookaround(a)) {
        return Err("unanchored prefix alternative with lookaround".into());
    }
    let body = loose.iter().map(|a| translate(a, cls)).collect::<Result<Vec<_>, _>>()?.join("|");
    Ok(Some(plain_regex(&body, false)?))
}

pub struct SplitAffixes {
    prefix: Vec<Run>,
    prefix_rest: Option<Regex>,
    suffix: Vec<Run>,
    infix: Vec<Run>,
    prefix_first: Option<CharSet>,
    suffix_last: Option<CharSet>,
    infix_first: Option<CharSet>,
    token_match: Option<WholeMatcher>,
    url_match: Option<WholeMatcher>,
}

impl SplitAffixes {
    pub fn new(r: &RulesJson, cls: &PyClasses) -> Result<Self, String> {
        let prefix_rest = r.prefix.as_deref().map(|p| unanchored_prefixes(p, cls)).transpose()?.flatten();
        let edge = |py: &Option<String>, first: bool| -> Result<Option<CharSet>, String> {
            let Some(py) = py else { return Ok(None) };
            Ok(edge_set(&translate(&strip_lookarounds(py), cls)?, first))
        };
        Ok(Self {
            prefix_first: if prefix_rest.is_some() { None } else { edge(&r.prefix, true)? },
            suffix_last: edge(&r.suffix, false)?,
            infix_first: edge(&r.infix, true)?,
            prefix: r.prefix.as_deref().map(|p| build_runs(p, cls, false)).transpose()?.unwrap_or_default(),
            prefix_rest,
            suffix: r.suffix.as_deref().map(|p| build_suffix(p, cls)).transpose()?.unwrap_or_default(),
            infix: r.infix.as_deref().map(|p| build_runs(p, cls, false)).transpose()?.unwrap_or_default(),
            token_match: WholeMatcher::new(&r.token_match, cls)?,
            url_match: WholeMatcher::new(&r.url_match, cls)?,
        })
    }

    fn prefix_at_start(&self, s: &str) -> Option<usize> {
        let first = s.chars().next();
        if !admits(&self.prefix_first, first) {
            return None;
        }
        self.prefix.iter().filter(|r| admits(r.edge(), first)).find_map(|run| match run {
            Run::Plain { at_start, .. } => at_start.find(s).map(|m| m.end()),
            Run::Look(l) => l.match_at(s, 0, true),
        })
    }
}

/// Edge-char prefilter: false only when the pattern provably cannot match.
#[inline]
fn admits(set: &Option<CharSet>, c: Option<char>) -> bool {
    match (set, c) {
        (None, _) => true,
        (Some(_), None) => false,
        (Some(f), Some(c)) => f.contains(c),
    }
}

impl Affixes for SplitAffixes {
    fn prefix_len(&self, s: &str) -> usize {
        if let Some(len) = self.prefix_at_start(s) {
            return len;
        }
        let Some(rest) = &self.prefix_rest else { return 0 };
        if s.is_empty() {
            return 0;
        }
        rest.find_at(s, next_char_boundary(s, 0)).map_or(0, |m| m.end() - m.start())
    }

    fn suffix_len(&self, s: &str) -> usize {
        let last = s.chars().next_back();
        if !admits(&self.suffix_last, last) {
            return 0;
        }
        let start = self.suffix.iter().filter(|r| admits(r.edge(), last)).filter_map(|run| match run {
            Run::Plain { find, .. } => find.find(s).map(|m| m.start()),
            Run::Look(l) => l.suffix_start(s),
        });
        start.min().map_or(0, |p| s.len() - p)
    }

    /// Python `finditer` (`_sre` scanner): after an empty match the next search starts at the same position but
    /// must not end there (`must_advance`); after a non-empty match an empty one may follow at its end.
    fn infixes(&self, s: &str, out: &mut Vec<(usize, usize)>) {
        out.clear();
        if let Some(f) = &self.infix_first {
            if !s.chars().any(|c| f.contains(c)) {
                return;
            }
        }
        // Per-run next candidate (start, len); still valid while start >= pos.
        let mut cand: [Option<Option<(usize, usize)>>; MAX_RUNS] = [None; MAX_RUNS];
        let (mut pos, mut must_advance) = (0, false);
        loop {
            let mut best: Option<(usize, usize)> = None;
            for (k, run) in self.infix.iter().enumerate() {
                let stale = match cand[k] {
                    None => true,
                    Some(Some((st, len))) => st < pos || (must_advance && st == pos && len == 0),
                    Some(None) => false,
                };
                if stale {
                    cand[k] = Some(run.find_from(s, pos, must_advance));
                }
                if let Some(Some((st, len))) = cand[k] {
                    if best.is_none_or(|(b, _)| st < b) {
                        best = Some((st, len));
                    }
                }
            }
            let Some((st, len)) = best else { break };
            out.push((st, st + len));
            must_advance = len == 0;
            pos = st + len;
        }
    }

    fn token_match(&self, s: &str) -> bool {
        WholeMatcher::is_match(&self.token_match, s)
    }

    fn url_match(&self, s: &str) -> bool {
        WholeMatcher::is_match(&self.url_match, s)
    }
}
