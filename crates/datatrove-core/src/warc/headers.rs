//! warcio's `StatusAndHeadersParser.parse`, for the inputs where it can't raise or depend on Unicode case mapping.

use crate::pystr::{lstrip, rstrip};
use std::borrow::Cow;

/// `StatusAndHeadersParser.decode_header`: UTF-8, else ISO-8859-1.
pub fn decode_line(b: &[u8]) -> Cow<'_, str> {
    match std::str::from_utf8(b) {
        Ok(s) => Cow::Borrowed(s),
        Err(_) => Cow::Owned(b.iter().map(|&c| c as char).collect()),
    }
}

/// A line that ends a header block: empty after `str.rstrip()` (EOF reads as an empty line too).
pub fn is_blank_line(b: &[u8]) -> bool {
    rstrip(&decode_line(b)).is_empty()
}

/// Parsed `(name, value)` pairs in order; lookups are case-insensitive and return the first match like `get_header`.
pub struct Headers(Vec<(String, String)>);

impl Headers {
    /// Header lines after the status line, excluding the terminating blank line. None when a name is not ASCII, where
    /// `str.lower()` matching could differ from ASCII case folding.
    pub fn parse<'a>(lines: impl IntoIterator<Item = &'a [u8]>) -> Option<Self> {
        let mut out = Vec::new();
        let mut current: Option<(String, Option<String>)> = None;
        for (i, raw) in lines.into_iter().enumerate() {
            let decoded = decode_line(raw);
            let line = rstrip(&decoded);
            if i > 0 && (line.starts_with(' ') || line.starts_with('\t')) {
                if let Some((_, Some(value))) = current.as_mut() {
                    value.push_str(line);
                }
                continue;
            }
            push(&mut out, current.take());
            let (name, value) = match line.split_once(':') {
                Some((n, v)) => (n.trim_end_matches([' ', '\t']), Some(lstrip(v).to_owned())),
                None => (line, None),
            };
            if !name.is_ascii() {
                return None;
            }
            current = Some((name.to_owned(), value));
        }
        push(&mut out, current);
        Some(Self(out))
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.0.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
}

fn push(out: &mut Vec<(String, String)>, header: Option<(String, Option<String>)>) {
    if let Some((name, Some(value))) = header {
        out.push((name, value));
    }
}

/// warcio's WARC status check (`split_prefix` over `WARC_TYPES`), restricted to ASCII status lines.
pub fn is_warc_status(line: &[u8]) -> bool {
    let decoded = decode_line(line);
    let s = rstrip(&decoded);
    s.is_ascii()
        && ["WARC/1.1", "WARC/1.0", "WARC/0.17", "WARC/0.18"]
            .iter()
            .any(|p| s.len() >= p.len() && s[..p.len()].eq_ignore_ascii_case(p))
}

/// Python `int(value)` for plain ASCII digit strings; None for anything warcio might parse differently.
pub fn parse_length(value: &str) -> Option<usize> {
    if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    value.parse().ok()
}

/// Splits `block` into `\n`-terminated lines like repeated `readline()` (the last one may lack the newline).
pub fn lines(block: &[u8]) -> impl Iterator<Item = &[u8]> {
    block.split_inclusive(|&b| b == b'\n')
}
