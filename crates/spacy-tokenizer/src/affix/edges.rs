//! Sets of chars a pattern can start/end with, used as exact negative prefilters: if the first
//! char of `s` is outside `first(prefix)`, `prefix_search(s)` cannot match, etc.

use crate::rules::CharSet;
use regex_syntax::hir::{Class, ClassUnicode, ClassUnicodeRange, Hir, HirKind};

/// (chars the match can begin/end with, whether the pattern can match empty).
fn edge(h: &Hir, first: bool) -> Option<(ClassUnicode, bool)> {
    let empty = ClassUnicode::empty();
    Some(match h.kind() {
        HirKind::Empty | HirKind::Look(_) => (empty, true),
        HirKind::Literal(lit) => {
            let s = std::str::from_utf8(&lit.0).ok()?;
            let c = if first { s.chars().next()? } else { s.chars().next_back()? };
            (ClassUnicode::new([ClassUnicodeRange::new(c, c)]), false)
        }
        HirKind::Class(Class::Unicode(cls)) => (cls.clone(), false),
        HirKind::Class(Class::Bytes(_)) => return None,
        HirKind::Capture(c) => edge(&c.sub, first)?,
        HirKind::Repetition(r) => {
            let (set, nullable) = edge(&r.sub, first)?;
            (set, nullable || r.min == 0)
        }
        HirKind::Alternation(v) => {
            let mut acc = empty;
            let mut nullable = false;
            for x in v {
                let (s, n) = edge(x, first)?;
                acc.union(&s);
                nullable |= n;
            }
            (acc, nullable)
        }
        HirKind::Concat(v) => {
            let mut acc = empty;
            let items: Box<dyn Iterator<Item = &Hir>> =
                if first { Box::new(v.iter()) } else { Box::new(v.iter().rev()) };
            for x in items {
                let (s, n) = edge(x, first)?;
                acc.union(&s);
                if !n {
                    return Some((acc, false));
                }
            }
            (acc, true)
        }
    })
}

/// None when no useful filter exists (pattern can match empty, or unsupported construct).
pub fn edge_set(rust_pattern: &str, first: bool) -> Option<CharSet> {
    let hir = regex_syntax::parse(rust_pattern).ok()?;
    let (set, nullable) = edge(&hir, first)?;
    if nullable {
        return None;
    }
    let ranges: Vec<(u32, u32)> = set.ranges().iter().map(|r| (r.start() as u32, r.end() as u32)).collect();
    Some(CharSet::new(&ranges))
}
