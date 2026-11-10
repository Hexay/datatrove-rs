use serde::Deserialize;

/// Output of `tools/export_rules.py`.
#[derive(Deserialize)]
pub struct RulesJson {
    pub spacy_version: String,
    pub faster_heuristics: bool,
    pub prefix: Option<String>,
    pub suffix: Option<String>,
    pub infix: Option<String>,
    pub token_match: Option<String>,
    pub url_match: Option<String>,
    pub rules: Vec<(String, Vec<String>)>,
    pub py_isspace: Vec<(u32, u32)>,
    pub py_w: Vec<(u32, u32)>,
    pub py_d: Vec<(u32, u32)>,
    pub py_s: Vec<(u32, u32)>,
}

impl std::str::FromStr for RulesJson {
    type Err = String;

    fn from_str(json: &str) -> Result<Self, String> {
        serde_json::from_str(json).map_err(|e| e.to_string())
    }
}

/// Python `re` Unicode class bodies (`\w`, `\d`, `\s`) as Rust class items, so translated
/// patterns keep Python semantics rather than Rust's slightly different Unicode definitions.
pub struct PyClasses {
    pub w: String,
    pub d: String,
    pub s: String,
}

impl PyClasses {
    pub fn new(r: &RulesJson) -> Self {
        Self { w: class_body(&r.py_w), d: class_body(&r.py_d), s: class_body(&r.py_s) }
    }
}

fn class_body(ranges: &[(u32, u32)]) -> String {
    ranges.iter().map(|&(lo, hi)| format!("\\x{{{lo:X}}}-\\x{{{hi:X}}}")).collect()
}

/// Sorted, non-overlapping code point ranges with an ASCII fast path. Used for Python
/// `str.isspace` (which differs from `char::is_whitespace`, e.g. U+001C..U+001F) and for the
/// first/last-char prefilters of the affix regexes.
pub struct CharSet {
    ascii: [bool; 128],
    ranges: Vec<(u32, u32)>,
}

impl CharSet {
    pub fn new(ranges: &[(u32, u32)]) -> Self {
        let mut ascii = [false; 128];
        for (c, slot) in ascii.iter_mut().enumerate() {
            *slot = ranges.iter().any(|&(lo, hi)| lo <= c as u32 && c as u32 <= hi);
        }
        Self { ascii, ranges: ranges.to_vec() }
    }

    #[inline]
    pub fn contains(&self, c: char) -> bool {
        let cp = c as u32;
        if cp < 128 {
            return self.ascii[cp as usize];
        }
        match self.ranges.binary_search_by(|&(lo, _)| lo.cmp(&cp)) {
            Ok(_) => true,
            Err(0) => false,
            Err(i) => cp <= self.ranges[i - 1].1,
        }
    }
}
