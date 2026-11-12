//! datatrove `PIIFormatter`: email and IPv4 replacement with the same matches, validation and round-robin choice.
//!
//! Python's `re` and the `regex` crate agree on leftmost-first matches for these patterns, except the email
//! pattern's leading `\b`, which uses Python's `\w` (`pyunicode::re_w`) and is checked here per start position.

use crate::pyunicode::py;
use regex::Regex;

const LOCAL_PUNCT: &str = "!#$%&'*+/=?^_`{|}~-";
const OCTET: &str = "(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)";

/// `ipaddress.ip_address(s).is_global` for every IPv4 address, plus the parser rules of the running Python.
pub struct IpRules {
    /// Sorted interval starts and whether addresses from there up to the next start are global.
    pub global_from: Vec<(u32, bool)>,
    /// Python >= 3.9.5 rejects octets with leading zeros.
    pub reject_leading_zeros: bool,
    /// `only_remove_public_ips`.
    pub public_only: bool,
}

impl IpRules {
    /// `public_ip_validator(s, public_only)` for a string matched by the IPv4 pattern.
    pub fn valid(&self, s: &str) -> bool {
        let mut ip = 0u32;
        for octet in s.split('.') {
            if self.reject_leading_zeros && octet.len() > 1 && octet.starts_with('0') {
                return false;
            }
            ip = ip << 8 | octet.parse::<u32>().unwrap_or(0);
        }
        if !self.public_only {
            return true;
        }
        match self.global_from.partition_point(|&(start, _)| start <= ip) {
            0 => false,
            i => self.global_from[i - 1].1,
        }
    }
}

pub struct Pii {
    email_tail: Regex,
    ip: Regex,
    rules: IpRules,
}

/// A round-robin replacement list; `next` is `PIIReplacer._replace_i`.
pub struct Replacements<'a> {
    pub items: &'a [String],
    pub next: usize,
}

impl Replacements<'_> {
    fn take(&mut self) -> &str {
        let r = &self.items[self.next];
        self.next = (self.next + 1) % self.items.len();
        r
    }
}

fn is_local(b: u8) -> bool {
    b.is_ascii_alphanumeric() || LOCAL_PUNCT.as_bytes().contains(&b)
}

fn is_word(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || (!c.is_ascii() && py().re_w.contains(c))
}

impl Pii {
    pub fn new(rules: IpRules) -> Self {
        let local = format!("[A-Za-z0-9{}]", regex::escape(LOCAL_PUNCT));
        let label = "[A-Za-z0-9](?:[A-Za-z0-9-]*[A-Za-z0-9])?";
        let email = format!(
            r"^{local}+(?:\.{local}+)*@(?:(?:{label}\.)+{label}|\[(?:{OCTET}\.){{3}}(?:{OCTET}|[A-Za-z0-9-]*[A-Za-z0-9]:)\])"
        );
        Self {
            email_tail: Regex::new(&email).expect("email pattern"),
            ip: Regex::new(&format!(r"(?:{OCTET}\.){{3}}{OCTET}")).expect("ip pattern"),
            rules,
        }
    }

    /// `emails_replacer.replace(text)`; returns None when nothing matched.
    pub fn replace_emails(&self, text: &str, repl: &mut Replacements) -> Option<String> {
        let b = text.as_bytes();
        let mut out = String::new();
        let (mut copied, mut p, mut run_end) = (0, 0, 0);
        while p < b.len() {
            if !is_local(b[p]) {
                p += 1;
                continue;
            }
            // A match starting in a run of local chars and dots needs '@' right after the run.
            if run_end <= p {
                run_end = p + b[p..].iter().position(|&c| !is_local(c) && c != b'.').unwrap_or(b.len() - p);
            }
            if b.get(run_end) != Some(&b'@') {
                p = run_end;
                continue;
            }
            if self.boundary(text, p) {
                if let Some(m) = self.email_tail.find(&text[p..]) {
                    out.push_str(&text[copied..p]);
                    out.push_str(repl.take());
                    copied = p + m.end();
                    p = copied;
                    continue;
                }
            }
            p += 1;
        }
        (copied > 0).then(|| out + &text[copied..])
    }

    /// Python `\b` before the ASCII char at byte `p`.
    fn boundary(&self, text: &str, p: usize) -> bool {
        let before = text[..p].chars().next_back().is_some_and(is_word);
        before != is_word(text.as_bytes()[p] as char)
    }

    /// `ip_replacer.replace(text)`; returns None when no match was replaced.
    pub fn replace_ips(&self, text: &str, repl: &mut Replacements) -> Option<String> {
        let mut out = String::new();
        let mut copied = 0;
        let mut replaced = false;
        for m in self.ip.find_iter(text) {
            if self.rules.valid(m.as_str()) {
                out.push_str(&text[copied..m.start()]);
                out.push_str(repl.take());
                copied = m.end();
                replaced = true;
            }
        }
        replaced.then(|| out + &text[copied..])
    }
}
