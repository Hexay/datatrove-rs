//! URLFilter's `filter`: the domain/URL blocklists datatrove loads into Python sets, the banned word / soft word /
//! subword checks in datatrove's order, and (via `tld`) tldextract's host split.

use super::strset::StrSet;
use super::tld::Suffixes;
use aho_corasick::AhoCorasick;
use rustc_hash::FxHashSet;

pub struct UrlLists {
    pub domains: StrSet,
    pub urls: StrSet,
    pub banned_words: FxHashSet<String>,
    pub soft_banned_words: FxHashSet<String>,
    /// None when there are no banned subwords (the check is skipped).
    pub subwords: Option<AhoCorasick>,
    pub soft_word_threshold: f64,
    /// The URLFilter's tldextract suffix list, when its behaviour is reproduced (see `tld`).
    pub suffixes: Option<Suffixes>,
}

fn is_sep(c: char) -> bool {
    !c.is_ascii_alphanumeric()
}

impl UrlLists {
    /// `subwords` must be non-empty normalized words; datatrove's automaton ignores empty ones.
    pub fn subword_matcher(subwords: &[String]) -> Option<AhoCorasick> {
        let words: Vec<&String> = subwords.iter().filter(|w| !w.is_empty()).collect();
        (!words.is_empty()).then(|| AhoCorasick::new(words).expect("valid patterns"))
    }

    /// The whole `filter`: Some(None) to keep, Some(reason), or None when Python's tldextract must split the host.
    pub fn check_url(&self, url: &str) -> Option<Option<&'static str>> {
        let (registered_domain, fqdn) = self.suffixes.as_ref()?.split(url)?;
        Some(self.check(url, &registered_domain, &fqdn))
    }

    /// The `filter` result after `tldextract`: None to keep, else the exclusion reason.
    pub fn check(&self, url: &str, registered_domain: &str, fqdn: &str) -> Option<&'static str> {
        if self.domains.contains(registered_domain) {
            return Some("domain");
        }
        if self.domains.contains(fqdn) {
            return Some("subdomain");
        }
        if self.urls.contains(url) {
            return Some("url");
        }
        // set(normalizer.split(url)): runs of non-[a-zA-Z0-9] split; a leading/trailing run yields ''
        let mut words: FxHashSet<&str> = url.split(is_sep).filter(|w| !w.is_empty()).collect();
        if url.starts_with(is_sep) || url.ends_with(is_sep) || url.is_empty() {
            words.insert("");
        }
        if words.iter().any(|w| self.banned_words.contains(*w)) {
            return Some("hard_blacklisted");
        }
        let soft = words.iter().filter(|w| self.soft_banned_words.contains(**w)).count() as f64;
        if soft >= self.soft_word_threshold {
            return Some("soft_blacklisted");
        }
        if let Some(ac) = &self.subwords {
            let normalized: String =
                url.chars().filter(char::is_ascii_alphanumeric).map(|c| c.to_ascii_lowercase()).collect();
            if ac.is_match(&normalized) {
                return Some("blacklisted_subword");
            }
        }
        None
    }
}
