//! One top-level alternative with a fixed-width lookbehind and/or a single-token lookahead: the core is
//! searched with `regex`, the lookarounds are checked by hand against the full string.

use super::edges::edge_set;
use super::{anchored, next_char_boundary, rx};
use crate::pyalt::{fixed_char_width, PyAlt};
use crate::pyre::translate;
use crate::rules::{CharSet, PyClasses};
use regex::Regex;
use regex_syntax::hir::{Hir, HirKind};

/// A core option: one possible length at a given start, or a greedy single-char repetition `c{min,}` whose
/// lengths Python's backtracking tries longest first when the lookahead fails.
enum Opt {
    Fixed(Regex),
    Greedy { char: Regex, min: u32 },
}

impl Opt {
    fn new(t: &str, lookahead: bool) -> Result<Self, String> {
        if !lookahead || single_length(t)? {
            return Ok(Opt::Fixed(rx(&format!("^(?:{t})"))?));
        }
        let hir = regex_syntax::parse(t).map_err(|e| e.to_string())?;
        match hir.kind() {
            HirKind::Repetition(r) if r.greedy && r.max.is_none() && matches!(r.sub.kind(), HirKind::Class(_)) => {
                Ok(Opt::Greedy { char: rx(&format!("^(?:{})", r.sub))?, min: r.min })
            }
            _ => Err(format!("lookahead after variable-length core {t}")),
        }
    }

    /// The first match length at the start of `s` accepted by `ok`, in Python's backtracking order.
    #[inline]
    fn first_len(&self, s: &str, ok: impl Fn(usize) -> bool) -> Option<usize> {
        match self {
            Opt::Fixed(re) => re.find(s).map(|m| m.end()).filter(|&len| ok(len)),
            Opt::Greedy { char, min } => {
                let mut ends = vec![0];
                while let Some(m) = char.find(&s[*ends.last().unwrap()..]) {
                    ends.push(ends.last().unwrap() + m.end());
                }
                ends.into_iter().skip(*min as usize).rev().find(|&len| ok(len))
            }
        }
    }
}

struct Lookbehind {
    width: usize,
    at_start: bool,
    re: Regex,
}

pub(super) struct LookAlt {
    lookbehind: Option<Lookbehind>,
    find: Regex,
    options: Vec<Opt>,
    lookahead: Vec<(bool, Regex)>,
    pub(super) edge: Option<CharSet>,
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
    pub(super) fn new(alt: &PyAlt, core: &str, cls: &PyClasses, suffix: bool) -> Result<Self, String> {
        let lookbehind = match &alt.lookbehind {
            None => None,
            Some(lb) => {
                // `(?<=^xy)` (lb): the window must also start the string.
                let (at_start, lb) = lb.strip_prefix('^').map_or((false, lb.as_str()), |rest| (true, rest));
                let w = fixed_char_width(lb).ok_or_else(|| format!("variable lookbehind {lb}"))?;
                Some(Lookbehind { width: w, at_start, re: rx(&format!("^(?:{})$", translate(lb, cls)?))? })
            }
        };
        let lookahead = alt
            .lookahead
            .iter()
            .map(|(pos, la)| Ok((*pos, rx(&anchored(la, cls)?)?)))
            .collect::<Result<Vec<_>, String>>()?;
        let mut options = Vec::new();
        for opt in alt.core_options() {
            options.push(Opt::new(&translate(&opt, cls)?, !lookahead.is_empty())?);
        }
        let core = translate(core, cls)?;
        let find = rx(&if suffix { format!("(?:{core})$") } else { format!("(?:{core})") })?;
        let edge = edge_set(&core, !suffix);
        Ok(Self { lookbehind, find, options, lookahead, edge })
    }

    #[inline]
    fn lookbehind_ok(&self, s: &str, pos: usize) -> bool {
        let Some(lb) = &self.lookbehind else { return true };
        let mut start = pos;
        for _ in 0..lb.width {
            match s[..start].chars().next_back() {
                Some(c) => start -= c.len_utf8(),
                None => return false,
            }
        }
        (!lb.at_start || start == 0) && lb.re.is_match(&s[start..pos])
    }

    /// Match length at `at`, the first core option (in pattern order) whose lookarounds hold; an empty match
    /// is skipped when `!allow_empty`, as Python backtracks past a match `finditer` must not repeat.
    #[inline]
    pub(super) fn match_at(&self, s: &str, at: usize, allow_empty: bool) -> Option<usize> {
        if !self.lookbehind_ok(s, at) {
            return None;
        }
        let ok = |len: usize| {
            (len != 0 || allow_empty)
                && self.lookahead.iter().all(|(positive, la)| la.is_match(&s[at + len..]) == *positive)
        };
        self.options.iter().find_map(|o| o.first_len(&s[at..], ok))
    }

    /// Leftmost (start, len) at or after `pos`; no empty match at `pos` itself when `must_advance`.
    pub(super) fn find_from(&self, s: &str, mut pos: usize, must_advance: bool) -> Option<(usize, usize)> {
        let origin = pos;
        while let Some(m) = self.find.find_at(s, pos) {
            if let Some(len) = self.match_at(s, m.start(), !(must_advance && m.start() == origin)) {
                return Some((m.start(), len));
            }
            if m.start() >= s.len() {
                return None;
            }
            pos = next_char_boundary(s, m.start());
        }
        None
    }

    pub(super) fn suffix_start(&self, s: &str) -> Option<usize> {
        let mut pos = 0;
        while let Some(m) = self.find.find_at(s, pos) {
            if self.lookbehind_ok(s, m.start()) {
                return Some(m.start());
            }
            if m.start() >= s.len() {
                return None;
            }
            pos = next_char_boundary(s, m.start());
        }
        None
    }
}
