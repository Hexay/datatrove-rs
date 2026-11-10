//! Python `str` methods with CPython's exact semantics (lengths are in code points).

use crate::pyunicode::py;

#[inline]
pub fn char_len(s: &str) -> usize {
    if s.is_ascii() {
        s.len()
    } else {
        s.chars().count()
    }
}

#[inline]
pub fn is_space(c: char) -> bool {
    py().isspace.contains(c)
}

pub fn strip(s: &str) -> &str {
    rstrip(lstrip(s))
}

pub fn lstrip(s: &str) -> &str {
    s.trim_start_matches(is_space)
}

pub fn rstrip(s: &str) -> &str {
    s.trim_end_matches(is_space)
}

/// `str.split()` with no arguments.
pub fn split_ws(s: &str) -> impl Iterator<Item = &str> {
    s.split(is_space).filter(|w| !w.is_empty())
}

/// `str.splitlines()`: breaks on Python's line boundaries, `\r\n` counts as one, no trailing empty item.
pub fn splitlines(s: &str) -> Vec<&str> {
    let t = py();
    let mut out = Vec::new();
    let mut start = 0;
    let mut it = s.char_indices().peekable();
    while let Some((i, c)) = it.next() {
        if t.linebreak.contains(c) {
            out.push(&s[start..i]);
            let mut end = i + c.len_utf8();
            if c == '\r' && matches!(it.peek(), Some(&(_, '\n'))) {
                it.next();
                end += 1;
            }
            start = end;
        }
    }
    if start < s.len() {
        out.push(&s[start..]);
    }
    out
}

/// `str.lower()`, including the context-sensitive final sigma.
pub fn lower(s: &str, out: &mut String) {
    out.clear();
    if s.is_ascii() {
        out.push_str(s);
        out.make_ascii_lowercase();
        return;
    }
    let t = py();
    for (i, c) in s.char_indices() {
        if c.is_ascii() {
            out.push(c.to_ascii_lowercase());
        } else if c == 'Σ' {
            out.push(if final_sigma(s, i) { 'ς' } else { 'σ' });
        } else {
            match t.lower_of(c) {
                Some(l) => out.push_str(l),
                None => out.push(c),
            }
        }
    }
}

/// CPython `handle_capital_sigma`: preceded by a cased char and not followed by one,
/// skipping case-ignorable chars in both directions.
fn final_sigma(s: &str, i: usize) -> bool {
    let t = py();
    let before = s[..i].chars().rev().find(|&c| !t.sigma_ignorable.contains(c));
    if !before.is_some_and(|c| t.sigma_cased.contains(c)) {
        return false;
    }
    let after = s[i + 'Σ'.len_utf8()..].chars().find(|&c| !t.sigma_ignorable.contains(c));
    !after.is_some_and(|c| t.sigma_cased.contains(c))
}

/// `"".join(c for c in unicodedata.normalize("NFD", s) if unicodedata.category(c) != "Mn")`.
pub fn nfd_without_mn(s: &str, buf: &mut Vec<char>, out: &mut String) {
    out.clear();
    if s.is_ascii() {
        out.push_str(s);
        return;
    }
    let t = py();
    buf.clear();
    for c in s.chars() {
        match t.nfd_of(c) {
            Some(d) => buf.extend(d.chars()),
            None => buf.push(c),
        }
    }
    canonical_order(buf);
    out.extend(buf.iter().filter(|&&c| !t.mn.contains(c)));
}

/// Stable-sorts each run of non-starters (ccc > 0) by combining class.
fn canonical_order(chars: &mut [char]) {
    let t = py();
    let mut i = 0;
    while i < chars.len() {
        if t.ccc(chars[i]) == 0 {
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len() && t.ccc(chars[i]) != 0 {
            i += 1;
        }
        if i - start > 1 {
            chars[start..i].sort_by_key(|&c| t.ccc(c));
        }
    }
}
