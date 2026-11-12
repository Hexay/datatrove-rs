//! FTFYFormatter's "`ftfy.fix_text` returns the text unchanged" pre-check. The checks themselves (ftfy's own
//! regexes, translated, and the keys of its translate tables) are built in `python/datatrove_rs/ftfy_check.py`.

use regex::{Regex, RegexBuilder};
use spacy_tokenizer_rs::rules::CharSet;

pub struct FtfyCheck {
    forbidden: CharSet,
    patterns: Vec<Regex>,
}

impl FtfyCheck {
    pub fn new(forbidden: &[(u32, u32)], patterns: &[String]) -> Result<Self, regex::Error> {
        let patterns = patterns
            .iter()
            .map(|p| RegexBuilder::new(p).size_limit(1 << 26).dfa_size_limit(1 << 26).build())
            .collect::<Result<_, _>>()?;
        Ok(Self { forbidden: CharSet::new(forbidden), patterns })
    }

    /// None when no check fires (the text is unchanged by ftfy); else 0 for a forbidden char or 1 + pattern index.
    pub fn first_hit(&self, text: &str) -> Option<usize> {
        if text.chars().any(|c| self.forbidden.contains(c)) {
            return Some(0);
        }
        self.patterns.iter().position(|p| p.is_match(text)).map(|i| i + 1)
    }
}
