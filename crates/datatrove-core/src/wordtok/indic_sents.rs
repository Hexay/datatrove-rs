//! `len(split_into_sentences(text))` for `IndicNLPTokenizer`: indic-nlp-library's `sentence_split`, then
//! datatrove's `strip_strings` and `simple_span_tokenize`, which raises when a merged sentence isn't in `text`.

use super::indic::{tables, Tables};
use super::SentenceCount;
use crate::pystr::{char_len, strip};

pub(super) fn sentence_count(text: &str, lang: &str) -> SentenceCount {
    let sents = sentence_split(text, lang);
    let sents: Vec<&str> = sents.iter().map(|s| strip(s)).filter(|s| !s.is_empty()).collect();
    if sents.len() <= 1 {
        return SentenceCount::Exact(1);
    }
    let mut start = 0;
    for s in &sents {
        match text[start..].find(s) {
            Some(off) => start += off + s.len(),
            None => return SentenceCount::Unknown,
        }
    }
    SentenceCount::Exact(sents.len())
}

/// Code-point lengths of `strip_strings(sentence_split(text, lang))`.
pub(super) fn sentence_lengths(text: &str, lang: &str) -> Vec<usize> {
    sentence_split(text, lang).iter().map(|s| char_len(strip(s))).filter(|&n| n > 0).collect()
}

fn sentence_split(text: &str, lang: &str) -> Vec<String> {
    let t = tables();
    let danda = t.danda_langs.iter().any(|l| l == lang) && text.chars().any(|c| t.contains_danda.contains(c));
    let delim = if danda { &t.delim_danda } else { &t.delim_no_danda };
    let text = strip(text);
    let mut cand: Vec<&str> = Vec::new();
    let mut begin = 0;
    let mut prev: Option<char> = None;
    for (p1, c) in text.char_indices() {
        if delim.contains(c) && !prev.is_some_and(|p| t.isnumeric.contains(p)) {
            let end = p1 + c.len_utf8();
            let s = strip(&text[begin..end]);
            if !s.is_empty() {
                cand.push(s);
            }
            begin = end;
        }
        prev = Some(c);
    }
    let s = strip(&text[begin..]);
    if !s.is_empty() {
        cand.push(s);
    }
    if danda {
        return cand.into_iter().map(str::to_owned).collect();
    }
    merge_abbreviations(&cand, lang, t)
}

/// sentence_split's second pass: glues single-word and acronym-final candidates to their neighbours with " ".
fn merge_abbreviations(cand: &[&str], lang: &str, t: &Tables) -> Vec<String> {
    let mut out = Vec::new();
    let mut buf = String::new();
    let mut bad = false;
    for &s in cand {
        let last = s.rsplit(' ').next().unwrap_or(s);
        let ends_dot = s.ends_with('.');
        if ends_dot && !s.contains(' ') {
            bad = true;
            buf.push(' ');
            buf.push_str(s);
        } else if ends_dot && is_acronym(&last[..last.len() - 1], lang, t) {
            if !buf.is_empty() && !bad {
                out.push(std::mem::take(&mut buf));
            }
            bad = true;
            buf = s.to_owned();
        } else if bad {
            buf.push(' ');
            buf.push_str(s);
            out.push(std::mem::take(&mut buf));
            bad = false;
        } else {
            if !buf.is_empty() {
                out.push(std::mem::take(&mut buf));
            }
            buf = s.to_owned();
            bad = false;
        }
    }
    if !buf.is_empty() {
        out.push(buf);
    }
    out
}

/// `is_acronym_abbvr`: the word transliterated to Devanagari is in the non-breaking list.
fn is_acronym(word: &str, lang: &str, t: &Tables) -> bool {
    let Some(&base) = t.script_base.get(lang) else {
        return t.ack_chars.contains(word);
    };
    let (base, sinhala) = if lang == "si" { (t.script_base["hi"], true) } else { (base, false) };
    let hi = t.script_base["hi"];
    let mut out = String::with_capacity(word.len());
    for c in word.chars() {
        let c = if sinhala { t.sinhala_to_devanagari.get(&c).copied().unwrap_or(c) } else { c };
        let cp = c as u32;
        let mapped = cp >= base && cp - base <= t.coordinated_end && c != '\u{964}' && c != '\u{965}';
        out.push(if mapped { char::from_u32(hi + cp - base).unwrap_or(c) } else { c });
    }
    t.ack_chars.contains(&out)
}
