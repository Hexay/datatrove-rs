//! ftfy's regexes, charmaps and HTML tables, exported by `tools/export_ftfy.py` from the version in
//! `ftfy_version`. Regexes are translated from ftfy's parse trees, so they match what Python's `re` matches.

use regex::{bytes, Regex, RegexBuilder};
use rustc_hash::{FxHashMap, FxHashSet};
use serde::Deserialize;
use spacy_tokenizer_rs::rules::CharSet;
use std::sync::OnceLock;

const JSON: &str = include_str!("../../data/ftfy.json");
pub const N_ENCODINGS: usize = 10;

#[derive(Deserialize)]
struct Raw {
    ftfy_version: String,
    python_version: String,
    unidata_version: String,
    badness: String,
    badness_lines: String,
    html_entity: String,
    ansi: String,
    utf8_detector: String,
    utf8_detector_behind: String,
    altered_utf8: String,
    lossy_utf8: String,
    a_grave_exceptions: Vec<String>,
    html_entities: FxHashMap<String, String>,
    invalid_charrefs: Vec<(u32, String)>,
    invalid_codepoints: Vec<u32>,
    control_chars: Vec<(u32, u32)>,
    encodings: Vec<RawEncoding>,
    windows_1252: Vec<Option<char>>,
    sloppy_windows_1252: Vec<Option<char>>,
}

#[derive(Deserialize)]
struct RawEncoding {
    name: String,
    chars: Vec<char>,
}

pub struct Encoding {
    pub name: String,
    pub sloppy: bool,
    pub macroman: bool,
}

pub struct Tables {
    pub ftfy_version: String,
    pub python_version: String,
    pub unidata_version: String,
    /// `BADNESS_RE` with `^` as `\A` (Python's `search` on one string).
    pub badness: Regex,
    /// `BADNESS_RE` with `^` at every line start: a superset prefilter over a whole multi-line text.
    pub badness_lines: Regex,
    pub html_entity: Regex,
    pub ansi: Regex,
    /// `UTF8_DETECTOR_RE` without its leading `(?<![behind])`, which `decode_inconsistent_utf8` checks by hand.
    pub utf8_detector: Regex,
    behind: Regex,
    pub altered_utf8: bytes::Regex,
    pub lossy_utf8: bytes::Regex,
    pub a_grave_exceptions: Vec<Vec<u8>>,
    pub html_entities: FxHashMap<String, String>,
    pub invalid_charrefs: FxHashMap<u32, String>,
    pub invalid_codepoints: FxHashSet<u32>,
    pub control_chars: CharSet,
    /// `chardata.CHARMAP_ENCODINGS`, in ftfy's order.
    pub encodings: Vec<Encoding>,
    /// Per char: bit i set iff encoding i can encode it, and its byte there.
    charmap: FxHashMap<char, (u16, [u8; N_ENCODINGS])>,
    pub windows_1252: Vec<Option<char>>,
    pub sloppy_windows_1252: Vec<Option<char>>,
}

fn rx(p: &str) -> Regex {
    RegexBuilder::new(p).size_limit(1 << 26).dfa_size_limit(1 << 26).build().expect("ftfy regex")
}

fn brx(p: &str) -> bytes::Regex {
    bytes::Regex::new(p).expect("ftfy bytes regex")
}

impl Tables {
    fn load() -> Self {
        let raw: Raw = serde_json::from_str(JSON).expect("ftfy.json");
        assert_eq!(raw.encodings.len(), N_ENCODINGS);
        let mut charmap: FxHashMap<char, (u16, [u8; N_ENCODINGS])> = FxHashMap::default();
        for (i, enc) in raw.encodings.iter().enumerate() {
            assert_eq!(enc.chars.len(), 256);
            for (b, &c) in enc.chars.iter().enumerate() {
                let e = charmap.entry(c).or_insert((0, [0; N_ENCODINGS]));
                e.0 |= 1 << i;
                e.1[i] = b as u8;
            }
        }
        let all = (1u16 << N_ENCODINGS) - 1;
        for b in (0u8..0x80).filter(|&b| b != 0x1a) {
            assert_eq!(charmap[&(b as char)], (all, [b; N_ENCODINGS]), "ASCII must encode as itself everywhere");
        }
        Self {
            badness: rx(&raw.badness),
            badness_lines: rx(&raw.badness_lines),
            html_entity: rx(&raw.html_entity),
            ansi: rx(&raw.ansi),
            utf8_detector: rx(&raw.utf8_detector),
            behind: rx(&format!(r"\A{}\z", raw.utf8_detector_behind)),
            altered_utf8: brx(&raw.altered_utf8),
            lossy_utf8: brx(&raw.lossy_utf8),
            a_grave_exceptions: raw.a_grave_exceptions.into_iter().map(String::into_bytes).collect(),
            html_entities: raw.html_entities,
            invalid_charrefs: raw.invalid_charrefs.into_iter().collect(),
            invalid_codepoints: raw.invalid_codepoints.into_iter().collect(),
            control_chars: CharSet::new(&raw.control_chars),
            encodings: raw
                .encodings
                .into_iter()
                .map(|e| Encoding {
                    sloppy: e.name.starts_with("sloppy"),
                    macroman: e.name == "macroman",
                    name: e.name,
                })
                .collect(),
            charmap,
            windows_1252: raw.windows_1252,
            sloppy_windows_1252: raw.sloppy_windows_1252,
            ftfy_version: raw.ftfy_version,
            python_version: raw.python_version,
            unidata_version: raw.unidata_version,
        }
    }

    /// Bit i set iff `possible_encoding(text, encodings[i])`.
    pub fn possible_encodings(&self, text: &str) -> u16 {
        let mut mask = (1u16 << N_ENCODINGS) - 1;
        for c in text.chars() {
            if c.is_ascii() && c != '\x1a' {
                continue;
            }
            mask &= self.charmap.get(&c).map_or(0, |e| e.0);
            if mask == 0 {
                break;
            }
        }
        mask
    }

    /// `text.encode(encodings[i])`; every char must be encodable (see `possible_encodings`).
    pub fn encode(&self, text: &str, i: usize) -> Vec<u8> {
        text.chars().map(|c| if c.is_ascii() && c != '\x1a' { c as u8 } else { self.charmap[&c].1[i] }).collect()
    }

    /// Whether `c` is in `UTF8_DETECTOR_RE`'s lookbehind class.
    pub fn detector_behind(&self, c: char) -> bool {
        self.behind.is_match(c.encode_utf8(&mut [0; 4]))
    }
}

pub fn tables() -> &'static Tables {
    static T: OnceLock<Tables> = OnceLock::new();
    T.get_or_init(Tables::load)
}
