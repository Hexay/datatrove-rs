//! Embedded rule sets: only languages whose tokens match spaCy on the differential corpus (docs/parity.md).

use crate::{Backend, Tokenizer};

/// Python's Unicode class tables, shared by every language (`tools/export_rules.py tables`).
pub const PY_TABLES: &str = include_str!("../data/py_tables.json");

macro_rules! rules {
    ($($lang:literal),* $(,)?) => {
        &[$(($lang, include_str!(concat!("../data/rules/", $lang, ".json")))),*]
    };
}

/// (spaCy language code, `tools/export_rules.py <lang>` output).
const RULES: &[(&str, &str)] = rules![
    "af", "am", "ar", "az", "bg", "ca", "cs", "da", "de", "dsb", "el", "en", "es", "et", "eu", "fa", "fi", "fo", "fr",
    "ga", "grc", "he", "hr", "hsb", "hu", "hy", "id", "is", "it", "ky", "la", "lb", "lg", "lij", "lt", "lv", "mk",
    "ms", "nb", "nl", "pl", "pt", "ro", "ru", "sk", "sl", "sq", "sr", "sv", "ti", "tl", "tn", "tr", "tt", "uk", "xx",
    "yo",
];

/// spaCy codes with an embedded rule set.
pub fn languages() -> impl Iterator<Item = &'static str> {
    RULES.iter().map(|&(lang, _)| lang)
}

/// `spacy.blank(lang).tokenizer`, or None when `lang` isn't embedded.
pub fn tokenizer(lang: &str, backend: Backend) -> Option<Result<Tokenizer, String>> {
    let (_, json) = RULES.iter().find(|&&(l, _)| l == lang)?;
    Some(Tokenizer::from_json(json, backend))
}

/// For the dev binaries: an embedded language code, or a path to an exported rules JSON not yet embedded.
pub fn load(spec: &str, backend: Backend) -> Result<Tokenizer, String> {
    if spec.ends_with(".json") {
        return Tokenizer::from_json(&std::fs::read_to_string(spec).map_err(|e| format!("{spec}: {e}"))?, backend);
    }
    tokenizer(spec, backend).ok_or_else(|| format!("no embedded rules for {spec}"))?
}
