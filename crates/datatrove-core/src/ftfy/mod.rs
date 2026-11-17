//! A port of `ftfy.fix_text` (ftfy 6.3.1) for the fixers datatrove's `FTFYFormatter` enables by default:
//! `unescape_html`, `fix_encoding` (with `restore_byte_a0`, `replace_lossy_sequences`, `decode_inconsistent_utf8`,
//! `fix_c1_controls`), `fix_c1_controls`, `remove_terminal_escapes` and `remove_control_chars`. `fix_surrogates`
//! is a no-op here: Rust strings can't hold surrogates and no step creates one. Configs that enable the other fixers
//! or a normalization, and other ftfy versions, are left to Python (`python/datatrove_rs/ftfy_check.py`).

mod encoding;
mod fixes;
mod tables;

pub use encoding::EncodingFlags;
pub use tables::{tables, Tables};

use std::borrow::Cow;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unescape {
    Off,
    On,
    /// `"auto"`: on until a segment contains `<`, then off for that segment and every later one.
    Auto,
}

#[derive(Clone, Copy, Debug)]
pub struct FixConfig {
    pub unescape_html: Unescape,
    pub remove_terminal_escapes: bool,
    pub fix_encoding: bool,
    /// What `fix_encoding` sees: the config's own flags, or the defaults when `explain` is off (ftfy then calls
    /// `fix_encoding(text)` without the config).
    pub encoding: EncodingFlags,
    pub fix_c1_controls: bool,
    pub remove_control_chars: bool,
    pub max_decode_length: usize,
}

/// `ftfy.fix_text(text, config)`, or None when it returns `text` unchanged.
pub fn fix_text(cfg: &FixConfig, text: &str) -> Option<String> {
    assert!(cfg.max_decode_length > 0);
    let t = tables();
    // With no segment cut inside a line, every segment starts a line, so the `^`-at-line-start badness regex
    // over the whole text matches wherever one segment's would.
    if text.len() <= cfg.max_decode_length
        && !needs_work(t, cfg, cfg.unescape_html != Unescape::Off, text, &t.badness_lines)
    {
        return None;
    }
    let mut unescape = cfg.unescape_html;
    let mut out: Option<String> = None;
    let mut pos = 0;
    while pos < text.len() {
        let end = segment_end(text, pos, cfg.max_decode_length);
        let seg = &text[pos..end];
        if unescape == Unescape::Auto && seg.contains('<') {
            unescape = Unescape::Off;
        }
        let fixed = fix_segment(t, cfg, unescape != Unescape::Off, seg);
        match (&mut out, fixed) {
            (Some(o), f) => o.push_str(&f),
            (None, Cow::Owned(f)) => out = Some(text[..pos].to_owned() + &f),
            (None, Cow::Borrowed(_)) => {}
        }
        pos = end;
    }
    out
}

/// End of the segment starting at `pos`: past the next `\n`, or `max_len` chars on.
fn segment_end(text: &str, pos: usize, max_len: usize) -> usize {
    let nl = memchr::memchr(b'\n', &text.as_bytes()[pos..]).map_or(text.len(), |i| pos + i + 1);
    if nl - pos <= max_len {
        return nl;
    }
    text[pos..nl].char_indices().nth(max_len).map_or(nl, |(i, _)| pos + i)
}

/// False only if every enabled fixer leaves `text` as it is (so `fix_and_explain` returns it after one round).
fn needs_work(t: &Tables, cfg: &FixConfig, unescape: bool, text: &str, badness: &regex::Regex) -> bool {
    (unescape && text.contains('&') && t.html_entity.is_match(text))
        || (cfg.fix_encoding && !text.is_ascii() && badness.is_match(text))
        || (cfg.fix_c1_controls && text.chars().any(encoding::is_c1))
        || (cfg.remove_terminal_escapes && text.contains('\x1b'))
        || (cfg.remove_control_chars && text.chars().any(|c| t.control_chars.contains(c)))
}

/// `fix_and_explain(segment, config)`: run the fixers in ftfy's order until a round changes nothing.
fn fix_segment<'a>(t: &Tables, cfg: &FixConfig, unescape: bool, seg: &'a str) -> Cow<'a, str> {
    if !needs_work(t, cfg, unescape, seg, &t.badness) {
        return Cow::Borrowed(seg);
    }
    let mut text = seg.to_owned();
    loop {
        let orig = text.clone();
        if unescape {
            apply(&mut text, |s| fixes::unescape_html(t, s));
        }
        if cfg.fix_encoding {
            apply(&mut text, |s| encoding::fix_encoding(t, s, &cfg.encoding));
        }
        if cfg.fix_c1_controls {
            apply(&mut text, |s| fixes::fix_c1_controls(t, s));
        }
        if cfg.remove_terminal_escapes {
            apply(&mut text, |s| fixes::remove_terminal_escapes(t, s));
        }
        if cfg.remove_control_chars {
            apply(&mut text, |s| fixes::remove_control_chars(t, s));
        }
        if text == orig {
            break;
        }
    }
    if text == seg {
        Cow::Borrowed(seg)
    } else {
        Cow::Owned(text)
    }
}

fn apply(text: &mut String, fixer: impl FnOnce(&str) -> Cow<'_, str>) {
    if let Cow::Owned(s) = fixer(text) {
        *text = s;
    }
}
