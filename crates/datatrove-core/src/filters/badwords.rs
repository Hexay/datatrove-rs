//! C4BadWordsFilter's regex search, `(?:\W|^)(w1|w2|...)(?:\W|$)` (or the bare alternation for ja/th/zh), over
//! already-lowercased text. Only whether a match exists matters, so this is "some occurrence of some word, flanked
//! by a Python non-`\w` char or the text edge on both sides" (`$` before a final '\n' is covered by '\n' being `\W`).

use crate::pyunicode::py;
use aho_corasick::AhoCorasick;

pub struct BadWords {
    ac: Option<AhoCorasick>,
    /// An empty word (from a blank line in the list) makes the alternation match the empty string.
    has_empty: bool,
    bounded: bool,
}

fn non_word(c: Option<char>) -> bool {
    match c {
        None => true,
        Some(c) if c.is_ascii() => !(c.is_ascii_alphanumeric() || c == '_'),
        Some(c) => !py().re_w.contains(c),
    }
}

impl BadWords {
    pub fn new(words: &[String], bounded: bool) -> Self {
        let nonempty: Vec<&str> = words.iter().map(String::as_str).filter(|w| !w.is_empty()).collect();
        Self {
            ac: (!nonempty.is_empty()).then(|| AhoCorasick::new(&nonempty).expect("valid patterns")),
            has_empty: nonempty.len() < words.len(),
            bounded,
        }
    }

    fn flanked(&self, text: &str, start: usize, end: usize) -> bool {
        non_word(text[..start].chars().next_back()) && non_word(text[end..].chars().next())
    }

    /// `badwords_regex.search(text) is not None`.
    pub fn search(&self, text: &str) -> bool {
        if self.has_empty {
            if !self.bounded {
                return true;
            }
            let mut edges = text.char_indices().map(|(i, _)| i).chain([text.len()]);
            if edges.any(|i| self.flanked(text, i, i)) {
                return true;
            }
        }
        let Some(ac) = &self.ac else { return false };
        if !self.bounded {
            return ac.is_match(text);
        }
        ac.find_overlapping_iter(text).any(|m| self.flanked(text, m.start(), m.end()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(ws: &[&str]) -> Vec<String> {
        ws.iter().map(|w| w.to_string()).collect()
    }

    #[test]
    fn boundaries_follow_python_word_chars() {
        let b = BadWords::new(&words(&["ass", "two words", "a*s"]), true);
        assert!(b.search("ass"));
        assert!(b.search("an ass."));
        assert!(!b.search("class"));
        assert!(!b.search("asses"));
        assert!(!b.search("_ass"));
        assert!(!b.search("éass"));
        assert!(!b.search("ass²"));
        assert!(b.search("ass\u{301}"), "combining marks are not \\w in Python");
        assert!(b.search("x two words!"));
        assert!(b.search("(a*s)"));
        assert!(b.search("bass ass"), "a later occurrence can match");
    }

    #[test]
    fn unbounded_and_empty_words() {
        assert!(BadWords::new(&words(&["ass"]), false).search("class"));
        assert!(!BadWords::new(&words(&[]), true).search("anything"));
        let e = BadWords::new(&words(&["", "zzz"]), true);
        assert!(e.search(""));
        assert!(e.search(" "));
        assert!(e.search("a  b"));
        assert!(!e.search("a b"));
        assert!(!e.search("ab"));
        assert!(BadWords::new(&words(&[""]), false).search("ab"));
    }
}
