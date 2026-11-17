//! ftfy's `fix_encoding_and_explain` / `_fix_encoding_one_step_and_explain` and the byte-level transcoders it uses.

use super::fixes::fix_c1_controls;
use super::tables::Tables;
use std::borrow::Cow;

/// The `TextFixerConfig` fields `fix_encoding` reads.
#[derive(Clone, Copy, Debug)]
pub struct EncodingFlags {
    pub restore_byte_a0: bool,
    pub replace_lossy_sequences: bool,
    pub decode_inconsistent_utf8: bool,
    pub fix_c1_controls: bool,
}

impl EncodingFlags {
    /// `TextFixerConfig()`'s values, which `ftfy.fix_encoding(text)` without a config uses.
    pub const DEFAULT: Self = Self {
        restore_byte_a0: true,
        replace_lossy_sequences: true,
        decode_inconsistent_utf8: true,
        fix_c1_controls: true,
    };
}

pub fn fix_encoding<'a>(t: &Tables, text: &'a str, f: &EncodingFlags) -> Cow<'a, str> {
    let mut cur = Cow::Borrowed(text);
    while let Some(next) = one_step(t, &cur, f) {
        if next == *cur {
            break;
        }
        cur = Cow::Owned(next);
    }
    cur
}

/// One `_fix_encoding_one_step_and_explain`; None when it returns the text unchanged.
fn one_step(t: &Tables, text: &str, f: &EncodingFlags) -> Option<String> {
    if text.is_ascii() || !t.badness.is_match(text) {
        return None;
    }
    let possible = t.possible_encodings(text);
    for (i, enc) in t.encodings.iter().enumerate() {
        if possible & (1 << i) == 0 {
            continue;
        }
        let mut b = t.encode(text, i);
        if f.restore_byte_a0 && !enc.macroman && t.altered_utf8.is_match(&b) {
            b = restore_byte_a0(t, &b);
        }
        if f.replace_lossy_sequences && enc.sloppy {
            if let Cow::Owned(r) = t.lossy_utf8.replace_all(&b, &b"\xef\xbf\xbd"[..]) {
                b = r;
            }
        }
        let decoded =
            if b.contains(&0xed) || b.contains(&0xc0) { decode_utf8_variants(&b) } else { String::from_utf8(b).ok() };
        if decoded.is_some() {
            return decoded;
        }
    }
    if f.decode_inconsistent_utf8 {
        if let Cow::Owned(fixed) = decode_inconsistent_utf8(t, text) {
            if fixed != text {
                return Some(fixed);
            }
        }
    }
    // `"windows-1252" in possible_1byte_encodings` is never true (ftfy only lists the sloppy codec), so text that
    // fits Latin-1 always tries `encode("latin-1").decode("windows-1252")`.
    if possible & 1 != 0 {
        debug_assert_eq!(t.encodings[0].name, "latin-1");
        let fixed: Option<String> = text.chars().map(|c| t.windows_1252[c as usize]).collect();
        if let Some(fixed) = fixed.filter(|x| x != text) {
            return Some(fixed);
        }
    }
    if f.fix_c1_controls && text.chars().any(is_c1) {
        return Some(fix_c1_controls(t, text).into_owned());
    }
    None
}

pub fn is_c1(c: char) -> bool {
    ('\u{80}'..='\u{9f}').contains(&c)
}

/// `fixes.restore_byte_a0`: `A_GRAVE_WORD_RE` (by hand: it has a lookahead), then `ALTERED_UTF8_RE`.
fn restore_byte_a0(t: &Tables, b: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(b.len() + 8);
    let mut i = 0;
    while i < b.len() {
        if b[i] == 0xc3
            && b.get(i + 1) == Some(&b' ')
            && !t.a_grave_exceptions.iter().any(|e| b[i + 2..].starts_with(e))
        {
            out.extend_from_slice(b"\xc3\xa0 ");
            i += 2;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    let replaced = t.altered_utf8.replace_all(&out, |c: &regex::bytes::Captures| {
        c[0].iter().map(|&x| if x == b' ' { 0xa0 } else { x }).collect::<Vec<u8>>()
    });
    replaced.into_owned()
}

/// `fixes.decode_inconsistent_utf8`: `UTF8_DETECTOR_RE.sub` with its one-char negative lookbehind checked on the
/// original text, as `re.sub` does.
fn decode_inconsistent_utf8<'a>(t: &Tables, text: &'a str) -> Cow<'a, str> {
    let n_chars = text.chars().count();
    let mut out = String::new();
    let (mut last, mut start) = (0, 0);
    while let Some(m) = t.utf8_detector.find_at(text, start) {
        if text[..m.start()].chars().next_back().is_some_and(|p| t.detector_behind(p)) {
            start = m.start() + text[m.start()..].chars().next().map_or(1, char::len_utf8);
            continue;
        }
        let sub = m.as_str();
        out.push_str(&text[last..m.start()]);
        if sub.chars().count() < n_chars && t.badness.is_match(sub) {
            out.push_str(&fix_encoding(t, sub, &EncodingFlags::DEFAULT));
        } else {
            out.push_str(sub);
        }
        (last, start) = (m.end(), m.end());
    }
    if last == 0 {
        return Cow::Borrowed(text);
    }
    out.push_str(&text[last..]);
    Cow::Owned(out)
}

/// `bytes.decode("utf-8-variants")` (ftfy's CESU-8 / Java-null decoder); None where it raises.
///
/// Mirrors `IncrementalDecoder._buffer_decode` with `final=True`: strict UTF-8 up to the next `SPECIAL_BYTES_RE`
/// match, which is `\xc0` or `\xed` followed by what its alternatives accept. Their `$` also matches before a
/// trailing `\n`, so `\xc0\n` at the end decodes to `\0` and swallows the newline, as in Python.
fn decode_utf8_variants(b: &[u8]) -> Option<String> {
    let n = b.len();
    let dollar = |p: usize| p == n || (p + 1 == n && b[p] == b'\n');
    let cont = |p: usize, lo: u8, hi: u8| p < n && (lo..=hi).contains(&b[p]);
    let special = |i: usize| match b[i] {
        0xc0 => cont(i + 1, 0x80, 0x80) || dollar(i + 1),
        0xed => (cont(i + 1, 0xa0, 0xbf) && (cont(i + 2, 0x80, 0xbf) || dollar(i + 2))) || dollar(i + 1),
        _ => false,
    };
    let mut out = String::with_capacity(n);
    let mut pos = 0;
    while pos < n {
        let Some(i) = (pos..n).find(|&i| special(i)) else {
            out.push_str(std::str::from_utf8(&b[pos..]).ok()?);
            break;
        };
        if i > pos {
            out.push_str(std::str::from_utf8(&b[pos..i]).ok()?);
            pos = i;
        } else if b[pos] == 0xc0 {
            if n - pos < 2 {
                return None;
            }
            out.push('\0');
            pos += 2;
        } else if n - pos < 6 {
            out.push_str(std::str::from_utf8(&b[pos..]).ok()?);
            break;
        } else if cont(pos + 1, 0xa0, 0xaf)
            && cont(pos + 2, 0x80, 0xbf)
            && b[pos + 3] == 0xed
            && cont(pos + 4, 0xb0, 0xbf)
            && (cont(pos + 5, 0x80, 0xbf) || dollar(pos + 5))
        {
            let cp = ((b[pos + 1] as u32 & 0x0f) << 16)
                + ((b[pos + 2] as u32 & 0x3f) << 10)
                + ((b[pos + 4] as u32 & 0x0f) << 6)
                + (b[pos + 5] as u32 & 0x3f)
                + 0x10000;
            out.push(char::from_u32(cp)?);
            pos += 6;
        } else {
            // Python decodes the next 3 bytes as plain UTF-8: \xed + [\xa0-\xbf] is a surrogate and raises.
            return None;
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::decode_utf8_variants as d;

    #[test]
    fn utf8_variants() {
        assert_eq!(d(b"a\xc0\x80b").as_deref(), Some("a\0b"));
        assert_eq!(d(b"a\xc0\n").as_deref(), Some("a\0"));
        assert_eq!(d(b"a\xc0"), None);
        assert_eq!(d(b"\xed\xa0\xbd\xed\xb2\xa9").as_deref(), Some("\u{1f4a9}"));
        assert_eq!(d(b"\xed\xa0\xbd\xed\xb2\n").as_deref(), Some("\u{1f48a}"));
        assert_eq!(d(b"\xed\x95\x9c").as_deref(), Some("\u{d55c}"));
        assert_eq!(d(b"\xed\xa0\x80xyz"), None);
        assert_eq!(d(b"\xe4\xc0\x80"), None);
    }
}
