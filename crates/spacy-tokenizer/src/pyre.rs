//! Python `re` syntax -> Rust `regex`/`fancy-regex` syntax.

use crate::rules::PyClasses;

fn hex_escape(c: char) -> String {
    format!("\\x{{{:X}}}", c as u32)
}

fn read_hex(chars: &[char], i: usize, n: usize) -> Result<char, String> {
    let s: String = chars.get(i..i + n).ok_or("truncated escape")?.iter().collect();
    let cp = u32::from_str_radix(&s, 16).map_err(|e| e.to_string())?;
    char::from_u32(cp).ok_or_else(|| format!("bad code point {cp:X}"))
}

/// Translate one escape starting after the backslash at `chars[i]`; returns (rust, consumed).
fn escape(chars: &[char], i: usize, cls: &PyClasses, in_class: bool) -> Result<(String, usize), String> {
    let c = *chars.get(i).ok_or("trailing backslash")?;
    Ok(match c {
        'u' => (hex_escape(read_hex(chars, i + 1, 4)?), 5),
        'U' => (hex_escape(read_hex(chars, i + 1, 8)?), 9),
        'x' => (hex_escape(read_hex(chars, i + 1, 2)?), 3),
        'w' => (format!("[{}]", cls.w), 1),
        'W' => (format!("[^{}]", cls.w), 1),
        'd' => (format!("[{}]", cls.d), 1),
        'D' => (format!("[^{}]", cls.d), 1),
        's' => (format!("[{}]", cls.s), 1),
        'S' => (format!("[^{}]", cls.s), 1),
        'n' | 't' | 'r' | 'f' | 'v' => (format!("\\{c}"), 1),
        'A' if !in_class => ("\\A".into(), 1),
        'Z' if !in_class => ("\\z".into(), 1),
        'b' if in_class => (hex_escape('\u{8}'), 1),
        c if c.is_alphanumeric() => return Err(format!("unsupported escape \\{c}")),
        c => (hex_escape(c), 1),
    })
}

/// Python `{m}`, `{m,}`, `{,n}`, `{m,n}` quantifier at `chars[i] == '{'`; None means literal `{`.
fn quantifier(chars: &[char], i: usize) -> Option<(String, usize)> {
    let close = chars[i..].iter().position(|&c| c == '}')? + i;
    let body: String = chars[i + 1..close].iter().collect();
    let (lo, hi) = match body.split_once(',') {
        Some((lo, hi)) => (lo, Some(hi)),
        None => (body.as_str(), None),
    };
    let digits = |s: &str| s.chars().all(|c| c.is_ascii_digit());
    if !digits(lo) || !hi.map_or(true, digits) || (lo.is_empty() && hi.map_or(true, str::is_empty)) {
        return None;
    }
    let lo = if lo.is_empty() { "0" } else { lo };
    let q = match hi {
        None => format!("{{{lo}}}"),
        Some(hi) => format!("{{{lo},{hi}}}"),
    };
    Some((q, close - i + 1))
}

/// Python-semantics pattern -> Rust syntax. Lookaround groups are passed through (fancy-regex
/// understands them; `regex` will reject them, which is the intended signal). Python's `$` also
/// matches before a trailing '\n'; kept as Rust `$` because chunks never mix '\n' with non-space.
pub fn translate(py: &str, cls: &PyClasses) -> Result<String, String> {
    let chars: Vec<char> = py.chars().collect();
    let mut out = String::with_capacity(py.len() * 2);
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '\\' => {
                let (s, n) = escape(&chars, i + 1, cls, false)?;
                out.push_str(&s);
                i += 1 + n;
            }
            '[' => i = translate_class(&chars, i, cls, &mut out)?,
            '{' => match quantifier(&chars, i) {
                Some((q, n)) => {
                    out.push_str(&q);
                    i += n;
                }
                None => {
                    out.push_str(&hex_escape('{'));
                    i += 1;
                }
            },
            '}' => {
                out.push_str(&hex_escape('}'));
                i += 1;
            }
            '(' if chars.get(i + 1) == Some(&'?') => {
                let rest: String = chars[i..chars.len().min(i + 5)].iter().collect();
                if rest.starts_with("(?u)") {
                    i += 4;
                } else if ["(?:", "(?=", "(?!", "(?<=", "(?<!", "(?P<"].iter().any(|p| rest.starts_with(p)) {
                    out.push('(');
                    i += 1;
                } else {
                    return Err(format!("unsupported group {rest}"));
                }
            }
            _ => {
                out.push(c);
                i += 1;
            }
        }
    }
    Ok(out)
}

/// Translate `[...]` starting at `chars[i] == '['`; returns index after the closing `]`.
fn translate_class(chars: &[char], mut i: usize, cls: &PyClasses, out: &mut String) -> Result<usize, String> {
    out.push('[');
    i += 1;
    if chars.get(i) == Some(&'^') {
        out.push('^');
        i += 1;
    }
    let mut first = true;
    let mut prev_atom = false;
    loop {
        let c = *chars.get(i).ok_or("unterminated class")?;
        match c {
            ']' if !first => {
                out.push(']');
                return Ok(i + 1);
            }
            '\\' => {
                let (s, n) = escape(chars, i + 1, cls, true)?;
                prev_atom = !s.starts_with('[');
                out.push_str(&s);
                i += 1 + n;
            }
            '-' if prev_atom && chars.get(i + 1).is_some_and(|&n| n != ']' && n != '-') => {
                out.push('-');
                prev_atom = false;
                i += 1;
            }
            // Characters that are set operators / nesting in Rust classes but literal in Python.
            '[' | ']' | '-' | '&' | '~' | '^' => {
                out.push_str(&hex_escape(c));
                prev_atom = true;
                i += 1;
            }
            _ => {
                out.push(c);
                prev_atom = true;
                i += 1;
            }
        }
        first = false;
    }
}
