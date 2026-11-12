//! affixcheck <strings.bin> <out.txt> [split|fancy] [lang|rules.json] — per '\0'-separated string, one line:
//! `prefix_len suffix_len url_match token_match infix_spans` in chars, for diffing against tools/affix_ref.py.

use spacy_tokenizer_rs::{char_offsets, langs, Backend};
use std::io::Write;

fn chars(s: &str, bytes: usize) -> usize {
    s[..bytes].chars().count()
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let backend = if a.get(3).map(String::as_str) == Some("fancy") { Backend::Fancy } else { Backend::Split };
    let raw = std::fs::read_to_string(&a[1]).unwrap();
    let tok = langs::load(a.get(4).map_or("en", String::as_str), backend).unwrap();
    let aff = tok.affixes();
    let mut w = std::io::BufWriter::new(std::fs::File::create(&a[2]).unwrap());
    let (mut inf, mut spans, mut offs) = (Vec::new(), Vec::new(), Vec::new());
    for s in raw.split('\0') {
        let pre = chars(s, aff.prefix_len(s));
        let suf = s.chars().count() - chars(s, s.len() - aff.suffix_len(s));
        aff.infixes(s, &mut inf);
        spans.clear();
        spans.extend(inf.iter().map(|&(x, y)| (x as u32, y as u32)));
        char_offsets(s, &spans, &mut offs);
        let infs: Vec<String> = offs.iter().map(|&(i, l)| format!("{}-{}", i, i + l)).collect();
        let (url, tm) = (aff.url_match(s) as u8, aff.token_match(s) as u8);
        writeln!(w, "{pre} {suf} {url} {tm} {}", infs.join(",")).unwrap();
    }
}
