//! ftfy's str -> str fixers that `fix_and_explain` runs around `fix_encoding`.

use super::encoding::is_c1;
use super::tables::Tables;
use regex::{Captures, NoExpand};
use std::borrow::Cow;

/// `fixes.unescape_html`: `HTML_ENTITY_RE.sub(_unescape_fixup, text)`.
pub fn unescape_html<'a>(t: &Tables, text: &'a str) -> Cow<'a, str> {
    t.html_entity.replace_all(text, |c: &Captures| unescape_entity(t, &c[0]))
}

fn unescape_entity(t: &Tables, ent: &str) -> String {
    if let Some(v) = t.html_entities.get(ent) {
        return v.clone();
    }
    let Some(body) = ent.strip_prefix("&#").and_then(|b| b.strip_suffix(';')) else {
        return ent.to_owned();
    };
    // `html.unescape(ent)`: only a whole decimal or `x`+hex body is replaced; anything else leaves a ";" behind,
    // and `_unescape_fixup` keeps the entity whenever the result contains ";".
    let num = if body.bytes().all(|b| b.is_ascii_digit()) {
        body.parse::<u128>().ok()
    } else {
        body.strip_prefix(['x', 'X'])
            .filter(|h| !h.is_empty() && h.bytes().all(|b| b.is_ascii_hexdigit()))
            .and_then(|h| u128::from_str_radix(h, 16).ok())
    };
    match num.map(|n| charref(t, n)) {
        Some(s) if !s.contains(';') => s,
        _ => ent.to_owned(),
    }
}

/// `html._replace_charref` for a numeric reference.
fn charref(t: &Tables, n: u128) -> String {
    if let Some(s) = u32::try_from(n).ok().and_then(|n| t.invalid_charrefs.get(&n)) {
        return s.clone();
    }
    if (0xd800..=0xdfff).contains(&n) || n > 0x10ffff {
        return "\u{fffd}".into();
    }
    let n = n as u32;
    if t.invalid_codepoints.contains(&n) {
        return String::new();
    }
    char::from_u32(n).expect("non-surrogate code point").into()
}

/// `fixes.fix_c1_controls`: each C1 char through `latin-1` -> `sloppy-windows-1252`.
pub fn fix_c1_controls<'a>(t: &Tables, text: &'a str) -> Cow<'a, str> {
    if !text.chars().any(is_c1) {
        return Cow::Borrowed(text);
    }
    Cow::Owned(
        text.chars()
            .map(
                |c| if is_c1(c) { t.sloppy_windows_1252[c as usize].expect("sloppy codec maps every byte") } else { c },
            )
            .collect(),
    )
}

/// `fixes.remove_terminal_escapes`.
pub fn remove_terminal_escapes<'a>(t: &Tables, text: &'a str) -> Cow<'a, str> {
    if !text.contains('\x1b') {
        return Cow::Borrowed(text);
    }
    t.ansi.replace_all(text, NoExpand(""))
}

/// `fixes.remove_control_chars`.
pub fn remove_control_chars<'a>(t: &Tables, text: &'a str) -> Cow<'a, str> {
    if !text.chars().any(|c| t.control_chars.contains(c)) {
        return Cow::Borrowed(text);
    }
    Cow::Owned(text.chars().filter(|&c| !t.control_chars.contains(c)).collect())
}
