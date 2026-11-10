//! Structural helpers on Python pattern text: top-level alternatives, and peeling fixed-width
//! lookarounds off an alternative (see `affix::split`).

fn utf8_width(b: u8) -> usize {
    match b {
        0..=0x7F => 1,
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        _ => 4,
    }
}

/// Index just past the group/class/escape that starts at byte `i` of `s`.
fn skip_atom(s: &[u8], i: usize) -> usize {
    match s[i] {
        b'\\' => i + 1 + utf8_width(s[i + 1]),
        b'[' => {
            let mut j = i + 1;
            if s.get(j) == Some(&b'^') {
                j += 1;
            }
            if s.get(j) == Some(&b']') {
                j += 1;
            }
            while s[j] != b']' {
                j = if s[j] == b'\\' { j + 2 } else { j + 1 };
            }
            j + 1
        }
        b'(' => {
            let mut j = i + 1;
            while s[j] != b')' {
                j = skip_atom(s, j);
            }
            j + 1
        }
        b => i + utf8_width(b),
    }
}

/// Split a pattern on its top-level `|`.
pub fn split_top(py: &str) -> Vec<&str> {
    let s = py.as_bytes();
    let (mut parts, mut start, mut i) = (Vec::new(), 0, 0);
    while i < s.len() {
        if s[i] == b'|' {
            parts.push(&py[start..i]);
            start = i + 1;
            i += 1;
        } else {
            i = skip_atom(s, i);
        }
    }
    parts.push(&py[start..]);
    parts
}

/// Top-level atoms (byte ranges) of an alternative, without quantifier awareness.
fn top_atoms(py: &str) -> Vec<(usize, usize)> {
    let s = py.as_bytes();
    let (mut v, mut i) = (Vec::new(), 0);
    while i < s.len() {
        let j = skip_atom(s, i);
        v.push((i, j));
        i = j;
    }
    v
}

const LOOKAROUNDS: [&str; 4] = ["(?=", "(?!", "(?<=", "(?<!"];

pub fn has_lookaround(py: &str) -> bool {
    LOOKAROUNDS.iter().any(|p| py.contains(p))
}

/// Delete every lookaround group; the result accepts a superset of the original language.
pub fn strip_lookarounds(py: &str) -> String {
    let s = py.as_bytes();
    let (mut out, mut i) = (String::with_capacity(py.len()), 0);
    while i < s.len() {
        let rest = &py[i..];
        if LOOKAROUNDS.iter().any(|p| rest.starts_with(p)) {
            i = skip_atom(s, i);
        } else if s[i] == b'(' {
            // Descend into ordinary groups so nested lookarounds are removed too.
            out.push('(');
            i += 1;
        } else {
            let j = skip_atom(s, i);
            out.push_str(&py[i..j]);
            i = j;
        }
    }
    out
}

/// One top-level alternative with its outer lookarounds peeled off (all still Python syntax).
#[derive(Debug, Clone)]
pub struct PyAlt {
    pub start_anchor: bool,
    pub end_anchor: bool,
    pub lookbehind: Option<String>,
    pub lookahead: Option<(bool, String)>,
    pub core: String,
}

impl PyAlt {
    pub fn parse(alt: &str) -> Self {
        let mut a = alt;
        let start_anchor = a.starts_with('^');
        if start_anchor {
            a = &a[1..];
        }
        let mut lookbehind = None;
        if a.starts_with("(?<=") {
            let end = skip_atom(a.as_bytes(), 0);
            lookbehind = Some(a[4..end - 1].to_string());
            a = &a[end..];
        }
        let atoms = top_atoms(a);
        let end_anchor = atoms.last().is_some_and(|&(s, e)| &a[s..e] == "$");
        if end_anchor {
            a = &a[..a.len() - 1];
        }
        let mut lookahead = None;
        if let Some(&(s, e)) = top_atoms(a).last() {
            let g = &a[s..e];
            if g.starts_with("(?=") || g.starts_with("(?!") {
                lookahead = Some((g.starts_with("(?="), g[3..g.len() - 1].to_string()));
                a = &a[..s];
            }
        }
        Self { start_anchor, end_anchor, lookbehind, lookahead, core: a.to_string() }
    }

    pub fn is_plain(&self) -> bool {
        self.lookbehind.is_none() && self.lookahead.is_none()
    }

    /// Core alternatives in priority order: a core that is one `(?:a|b|..)` group is expanded so
    /// the caller can emulate Python backtracking from one branch into the next on lookahead failure.
    pub fn core_options(&self) -> Vec<String> {
        let atoms = top_atoms(&self.core);
        if atoms.len() == 1 && self.core.starts_with("(?:") {
            return split_top(&self.core[3..self.core.len() - 1]).into_iter().map(String::from).collect();
        }
        vec![self.core.clone()]
    }
}

/// Number of chars matched by a lookbehind made only of single-char atoms; None if not fixed.
pub fn fixed_char_width(py: &str) -> Option<usize> {
    let mut n = 0;
    for (s, e) in top_atoms(py) {
        let atom = &py[s..e];
        let single = atom.starts_with('[')
            || (atom.starts_with('\\') && !matches!(atom.as_bytes()[1], b'u' | b'U' | b'x'))
            || atom.chars().count() == 1 && !"()|*+?{}^$.".contains(atom);
        if !single {
            return None;
        }
        n += 1;
    }
    Some(n)
}
