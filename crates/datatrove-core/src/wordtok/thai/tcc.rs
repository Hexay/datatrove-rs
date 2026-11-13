//! pythainlp `tcc_p.tcc_pos_array`: Thai Character Cluster boundaries, from the same regex.

use regex::Regex;
use std::sync::OnceLock;

const RULES: &str = "เc็ck เcctาะk เccีtยะk เccีtยLk เcc็ck เcิc์ck เcิtck เcีtยะ?k เcืtอะ?k \
                     เc[ิีุู]tยLk เctา?ะ?k cัtวะk c[ัื]tc[ุิะ]?k c[ิุู]์ c[ะ-ู]tk cรรc์ c็ ct[ะาำ]?k ck แc็c \
                     แcc์ แctะ แcc็c แccc์ โctะ [เ-ไ]ct ก็ อึ หึ";

const K: &str = "[ก-ฮ][ก-ฮ]?[ูุิ]?[์]";

fn pattern() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        // `L` is tcc_p's `(?=[เ-ไก-ฮ]|$)` before a greedy optional `k`: either `k` matches (its first char
        // satisfies the lookahead) or the lookahead char is captured and cut off in `cluster_len`.
        // Python's `$` also matches before a final "\n".
        let alts = RULES
            .replace("Lk", &format!("(?:{K}|([เ-ไก-ฮ]|\\n?\\z))"))
            .replace('k', &format!("(?:{K})?"))
            .replace('c', "[ก-ฮ]")
            .replace('t', "[่-๋]?");
        let alts: Vec<&str> = alts.split_whitespace().collect();
        Regex::new(&format!("^(?:{})", alts.join("|"))).expect("TCC pattern")
    })
}

/// Captures run the slow backtracker, so only when a lookahead char may have been consumed.
fn cluster_len(re: &Regex, rest: &str) -> Option<usize> {
    let end = re.find(rest)?.end();
    let last = rest[..end].chars().next_back().expect("non-empty");
    if end < rest.len() && !matches!(last, '\n' | 'ก'..='ฮ' | 'เ'..='ไ') {
        return Some(end);
    }
    let caps = re.captures(rest).expect("matched");
    Some(caps.get(1).or_else(|| caps.get(2)).map_or(end, |g| g.start()))
}

/// Every TCC alternative starts with one of these.
fn can_start(c: char) -> bool {
    ('\u{E01}'..='\u{E2E}').contains(&c) || ('\u{E40}'..='\u{E44}').contains(&c)
}

/// `out[i]` is true when char offset `i` ends a TCC; `out.len() == chars + 1`.
pub fn valid_positions(text: &str, out: &mut Vec<bool>) {
    out.clear();
    out.push(false);
    let re = pattern();
    let mut rest = text;
    while let Some(c) = rest.chars().next() {
        let m = if can_start(c) { cluster_len(re, rest) } else { None };
        let len = m.unwrap_or(c.len_utf8());
        let n = rest[..len].chars().count();
        out.extend(std::iter::repeat_n(false, n - 1));
        out.push(true);
        rest = &rest[len..];
    }
}
