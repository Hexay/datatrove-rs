//! Chinese and Japanese tokenization exactly as datatrove's `SpaCyTokenizer("zh")` / `SpaCyTokenizer("ja")` build
//! their spaCy `Doc`s: token byte spans, including the whitespace tokens spaCy keeps.

pub mod align;
pub mod jieba;
pub mod japanese;

/// (start, end) byte offsets of a token.
pub type Span = (u32, u32);

/// Python's `str.isspace` for one char.
pub fn py_isspace(c: char) -> bool {
    matches!(c, '\t'..='\r' | '\x1c'..='\x1f' | ' ' | '\u{85}' | '\u{a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}'
        | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}')
}

/// Python's `s.isspace()`: non-empty and all whitespace.
pub fn py_str_isspace(s: &str) -> bool {
    !s.is_empty() && s.chars().all(py_isspace)
}
