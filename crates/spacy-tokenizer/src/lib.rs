pub mod affix;
pub mod pyalt;
pub mod pyre;
pub mod rules;
pub mod specials;
pub mod tokenizer;

pub use affix::Backend;
pub use tokenizer::{State, Tokenizer};

/// Rules exported from `spacy.blank("en")` by `tools/export_rules.py`.
pub const EN_RULES: &str = include_str!("../data/en_rules.json");

pub fn english(backend: Backend) -> Tokenizer {
    Tokenizer::from_json(EN_RULES, backend).expect("embedded English rules")
}

/// Byte spans -> (char_start, char_len), the units of spaCy's `token.idx` / `len(token.text)`.
pub fn char_offsets(text: &str, spans: &[(u32, u32)], out: &mut Vec<(u32, u32)>) {
    out.clear();
    let mut chars = 0u32;
    let mut byte = 0usize;
    let mut advance = |to: usize, chars: &mut u32| {
        *chars += text[byte..to].chars().count() as u32;
        byte = to;
    };
    for &(a, b) in spans {
        advance(a as usize, &mut chars);
        let start = chars;
        advance(b as usize, &mut chars);
        out.push((start, chars - start));
    }
}
