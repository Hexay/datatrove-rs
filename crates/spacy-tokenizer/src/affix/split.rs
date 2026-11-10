//! `regex`-crate backend. Each top-level alternative with a fixed-width lookbehind and/or a
//! single-token lookahead becomes a `LookAlt` whose core is searched with `regex` and whose
//! lookarounds are checked against the full string, reproducing Python's backtracking order:
//! alternatives are tried in pattern order at the leftmost position (prefix/infix), and for
//! `$`-anchored suffixes the result is simply the leftmost start over all alternatives.

use super::edges::edge_set;
use super::whole::WholeMatcher;
use super::{anchored, next_char_boundary, rx, Affixes};
use crate::pyalt::{fixed_char_width, has_lookaround, split_top, strip_lookarounds, PyAlt};
use crate::pyre::translate;
use crate::rules::{CharSet, PyClasses, RulesJson};
use regex::Regex;
use regex_syntax::hir::{Hir, HirKind};

const MAX_RUNS: usize = 16;

struct LookAlt {
    lookbehind: Option<(usize, Regex)>,
    find: Regex,
    options: Vec<Regex>,
    lookahead: Option<(bool, Regex)>,
    edge: Option<CharSet>,
}

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
}

/// True when the pattern can match at most one length at a given start (no alternation or
/// repetition), so a failed lookahead cannot be rescued by backtracking into the core.
fn single_length(t: &str) -> Result<bool, String> {
    fn walk(h: &Hir) -> bool {
        match h.kind() {
            HirKind::Alternation(_) | HirKind::Repetition(_) => false,
            HirKind::Concat(v) => v.iter().all(walk),
            HirKind::Capture(c) => walk(&c.sub),
            _ => true,
        }
    }
    Ok(walk(&regex_syntax::parse(t).map_err(|e| e.to_string())?))
}

impl LookAlt {
    fn new(alt: &PyAlt, core: &str, cls: &PyClasses, suffix: bool) -> Result<Self, String> {
        let lookbehind = match &alt.lookbehind {
            None => None,
            Some(lb) => {
                let w = fixed_char_width(lb).ok_or_else(|| format!("variable lookbehind {lb}"))?;
                Some((w, rx(&format!("^(?:{})$", translate(lb, cls)?))?))
            }
        };
        let lookahead = match &alt.lookahead {
            None => None,
            Some((pos, la)) => Some((*pos, rx(&anchored(la, cls)?)?)),
        };
        let mut options = Vec::new();
        for opt in alt.core_options() {
            let t = translate(&opt, cls)?;
            if lookahead.is_some() && !single_length(&t)? {
                return Err(format!("lookahead after variable-length core {opt}"));
            }
            options.push(rx(&format!("^(?:{t})"))?);
        }
        let core = translate(core, cls)?;
        let find = rx(&if suffix { format!("(?:{core})$") } else { format!("(?:{core})") })?;
        let edge = edge_set(&core, !suffix);
        Ok(Self { lookbehind, find, options, lookahead, edge })
    }

    #[inline]
    fn lookbehind_ok(&self, s: &str, pos: usize) -> bool {
        let Some((w, re)) = &self.lookbehind else { return true };
        let mut start = pos;
        for _ in 0..*w {
            match s[..start].chars().next_back() {
                Some(c) => start -= c.len_utf8(),
                None => return false,
            }
        }
        re.is_match(&s[start..pos])
    }

    #[inline]
    fn match_at(&self, s: &str, at: usize) -> Option<usize> {
        if !self.lookbehind_ok(s, at) {
            return None;
        }
        self.options.iter().find_map(|o| {
            let len = o.find(&s[at..])?.end();
            match &self.lookahead {
                Some((positive, la)) if la.is_match(&s[at + len..]) != *positive => None,
                _ => Some(len),
            }
        })
    }

    fn find_from(&self, s: &str, mut pos: usize) -> Option<(usize, usize)> {
        while let Some(m) = self.find.find_at(s, pos) {
            if let Some(len) = self.match_at(s, m.start()) {
                return Some((m.start(), len));
            }
            pos = next_char_boundary(s, m.start());
        }
        None
    }

    fn suffix_start(&self, s: &str) -> Option<usize> {
        let mut pos = 0;
        while let Some(m) = self.find.find_at(s, pos) {
            if self.lookbehind_ok(s, m.start()) {
                return Some(m.start());
            }
            pos = next_char_boundary(s, m.start());
        }
        None
    }
}

/// Group consecutive lookaround-free alternatives into one `regex`, preserving priority order.
fn build_runs(py: &str, cls: &PyClasses, suffix: bool) -> Result<Vec<Run>, String> {
    let alts: Vec<PyAlt> = split_top(py).into_iter().map(PyAlt::parse).collect();
    let mut runs = Vec::new();
    let mut plain: Vec<String> = Vec::new();
    let flush = |plain: &mut Vec<String>, runs: &mut Vec<Run>| -> Result<(), String> {
        if !plain.is_empty() {
            let body = plain.join("|");
            let find = rx(&if suffix { format!("(?:{body})$") } else { format!("(?:{body})") })?;
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
/// share a lookbehind are merged into one core regex.
fn build_suffix(py: &str, cls: &PyClasses) -> Result<Vec<Run>, String> {
    let alts: Vec<PyAlt> = split_top(py).into_iter().map(PyAlt::parse).collect();
    if alts.iter().any(|a| !a.end_anchor || a.start_anchor || a.lookahead.is_some()) {
        return Err("suffix alternative not of form (?<=x)y$".into());
    }
    let mut groups: Vec<(Option<String>, Vec<String>)> = Vec::new();
    for a in &alts {
        match groups.iter_mut().find(|(lb, _)| *lb == a.lookbehind) {
            Some((_, cores)) => cores.push(a.core.clone()),
            None => groups.push((a.lookbehind.clone(), vec![a.core.clone()])),
        }
    }
    let mut runs = Vec::new();
    for (lb, cores) in groups {
        let core = format!("(?:{})", cores.join("|"));
        let alt = PyAlt { start_anchor: false, end_anchor: true, lookbehind: lb, lookahead: None, core: core.clone() };
        runs.push(Run::Look(LookAlt::new(&alt, &core, cls, true)?));
    }
    Ok(runs)
}

pub struct SplitAffixes {
    prefix: Vec<Run>,
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
        let prefix = r.prefix.as_deref().map(|p| build_runs(p, cls, false)).transpose()?.unwrap_or_default();
        if r.prefix.as_deref().is_some_and(|p| split_top(p).iter().any(|a| !a.starts_with('^'))) {
            return Err("prefix alternative without ^".into());
        }
        let edge = |py: &Option<String>, first: bool| -> Result<Option<CharSet>, String> {
            let Some(py) = py else { return Ok(None) };
            Ok(edge_set(&translate(&strip_lookarounds(py), cls)?, first))
        };
        Ok(Self {
            prefix_first: edge(&r.prefix, true)?,
            suffix_last: edge(&r.suffix, false)?,
            infix_first: edge(&r.infix, true)?,
            prefix,
            suffix: r.suffix.as_deref().map(|p| build_suffix(p, cls)).transpose()?.unwrap_or_default(),
            infix: r.infix.as_deref().map(|p| build_runs(p, cls, false)).transpose()?.unwrap_or_default(),
            token_match: WholeMatcher::new(&r.token_match, cls)?,
            url_match: WholeMatcher::new(&r.url_match, cls)?,
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
        let first = s.chars().next();
        if !admits(&self.prefix_first, first) {
            return 0;
        }
        for run in self.prefix.iter().filter(|r| admits(r.edge(), first)) {
            let len = match run {
                Run::Plain { at_start, .. } => at_start.find(s).map(|m| m.end()),
                Run::Look(l) => l.match_at(s, 0),
            };
            if let Some(len) = len {
                return len;
            }
        }
        0
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

    fn infixes(&self, s: &str, out: &mut Vec<(usize, usize)>) {
        out.clear();
        if let Some(f) = &self.infix_first {
            if !s.chars().any(|c| f.contains(c)) {
                return;
            }
        }
        // Per-run next candidate (start, len); still valid while start >= pos.
        let mut cand: [Option<Option<(usize, usize)>>; MAX_RUNS] = [None; MAX_RUNS];
        let mut pos = 0;
        loop {
            let mut best: Option<(usize, usize)> = None;
            for (k, run) in self.infix.iter().enumerate() {
                let stale = match cand[k] {
                    None => true,
                    Some(Some((st, _))) => st < pos,
                    Some(None) => false,
                };
                if stale {
                    cand[k] = Some(match run {
                        Run::Plain { find, .. } => find.find_at(s, pos).map(|m| (m.start(), m.end() - m.start())),
                        Run::Look(l) => l.find_from(s, pos),
                    });
                }
                if let Some(Some((st, len))) = cand[k] {
                    if best.is_none_or(|(b, _)| st < b) {
                        best = Some((st, len));
                    }
                }
            }
            match best {
                Some((st, len)) if len > 0 => {
                    out.push((st, st + len));
                    pos = st + len;
                }
                Some(_) => panic!("empty infix match unsupported"),
                None => break,
            }
        }
    }

    fn token_match(&self, s: &str) -> bool {
        WholeMatcher::is_match(&self.token_match, s)
    }

    fn url_match(&self, s: &str) -> bool {
        WholeMatcher::is_match(&self.url_match, s)
    }
}
