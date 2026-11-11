//! The set datatrove's `get_list(..., do_normalize=False)` builds, for lists with millions of lines.

use crate::pystr::strip;
use rustc_hash::FxHashSet;

/// Entries of a sorted list file are binary-searched in place (loading is one pass, no hashing); an unsorted file
/// falls back to a hash set. `extra` entries always go to the hash set.
pub struct StrSet {
    text: String,
    sorted: Vec<(u32, u32)>,
    hashed: FxHashSet<Box<str>>,
}

impl StrSet {
    /// `parse_list(open(path), do_normalize=False) | extra`: text-mode lines (universal newlines), skipping lines
    /// whose first character is `#`, each `str.strip()`ped.
    pub fn from_list_file(text: String, extra: &[String]) -> Self {
        let mut sorted = Vec::with_capacity(memchr::memchr_iter(b'\n', text.as_bytes()).count() + 1);
        let mut in_order = text.len() <= u32::MAX as usize;
        let mut prev = "";
        for line in text_lines(&text).filter(|l| !l.starts_with('#')) {
            let entry = strip(line);
            in_order &= prev <= entry;
            prev = entry;
            sorted.push(((entry.as_ptr() as usize - text.as_ptr() as usize) as u32, entry.len() as u32));
        }
        let mut hashed: FxHashSet<Box<str>> = extra.iter().map(|s| s.as_str().into()).collect();
        if !in_order {
            hashed.extend(sorted.drain(..).map(|(s, l)| text[s as usize..(s + l) as usize].into()));
            return Self { text: String::new(), sorted, hashed };
        }
        Self { text, sorted, hashed }
    }

    fn get(&self, (start, len): (u32, u32)) -> &str {
        &self.text[start as usize..(start + len) as usize]
    }

    pub fn contains(&self, s: &str) -> bool {
        self.hashed.contains(s) || self.sorted.binary_search_by(|&e| self.get(e).cmp(s)).is_ok()
    }

    /// Entries including duplicates of the file and `extra`; for diagnostics.
    pub fn len(&self) -> usize {
        self.sorted.len() + self.hashed.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Lines as Python's text-mode file iteration sees them, minus the line ending (`\n`, `\r\n` or `\r`).
fn text_lines(text: &str) -> impl Iterator<Item = &str> {
    let mut rest = text;
    std::iter::from_fn(move || {
        if rest.is_empty() {
            return None;
        }
        let end = memchr::memchr2(b'\n', b'\r', rest.as_bytes()).unwrap_or(rest.len());
        let line = &rest[..end];
        let skip = if rest[end..].starts_with("\r\n") { 2 } else { (end < rest.len()) as usize };
        rest = &rest[end + skip..];
        Some(line)
    })
}

#[cfg(test)]
mod tests {
    use super::StrSet;

    #[test]
    fn python_line_semantics_sorted_or_not() {
        for text in ["# c\r\n\na.com\rb.com \n c.com\n#d.com\n", "c.com\n#x\nb.com \ra.com\r\n\n"] {
            let set = StrSet::from_list_file(text.to_owned(), &["e.com".to_owned()]);
            for present in ["a.com", "b.com", "c.com", "", "e.com"] {
                assert!(set.contains(present), "{present:?} in {text:?}");
            }
            for absent in ["#d.com", "d.com", "# c", "#x", " c.com", "b.com "] {
                assert!(!set.contains(absent), "{absent:?} in {text:?}");
            }
        }
    }
}
