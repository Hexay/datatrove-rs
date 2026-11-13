//! spaCy's `util.get_words_and_spaces` + `Doc(words, spaces)`, as token byte spans of `text`.

use crate::{py_isspace, py_str_isspace, Span};

/// `"".join("".join(words).split()) == "".join(text.split())`: same non-whitespace chars in the same order.
pub fn same_non_space<'a>(text: &str, words: impl IntoIterator<Item = &'a str>) -> bool {
    let mut t = text.chars().filter(|&c| !py_isspace(c));
    for w in words {
        for c in w.chars().filter(|&c| !py_isspace(c)) {
            if t.next() != Some(c) {
                return false;
            }
        }
    }
    t.next().is_none()
}

/// Appends the doc tokens for `words` aligned on `text`, offset by `base`: each non-whitespace word, the gap
/// before it, and the trailing rest; a single ' ' after a word is that token's trailing space.
/// `space_after(i)` decides whether word `i`'s trailing space is taken (spaCy checks the text, datatrove's
/// Japanese tokenizer checks the next word). Err when a word is not found, where spaCy raises E194.
pub fn align(
    text: &str,
    words: &[&str],
    base: u32,
    space_after: impl Fn(usize, &str, usize) -> bool,
    out: &mut Vec<Span>,
) -> Result<(), String> {
    let mut pos = 0usize;
    let mut push = |a: usize, b: usize| out.push((base + a as u32, base + b as u32));
    for (i, w) in words.iter().enumerate() {
        if py_str_isspace(w) {
            continue;
        }
        let start = pos + text[pos..].find(w).ok_or_else(|| format!("E194: {w:?} not found"))?;
        if start > pos {
            push(pos, start);
        }
        push(start, start + w.len());
        pos = start + w.len();
        if space_after(i, text, pos) {
            pos += 1;
        }
    }
    if pos < text.len() {
        push(pos, text.len());
    }
    Ok(())
}

/// spaCy's rule: the char after the word is ' '.
pub fn text_space(_: usize, text: &str, pos: usize) -> bool {
    text.as_bytes().get(pos) == Some(&b' ')
}
