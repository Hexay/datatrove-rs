//! tldextract 5.4's `registered_domain` and `fqdn` for ASCII hosts; anything else returns None for Python to decide.

use crate::pystr::strip;
use rustc_hash::FxHashMap;

#[derive(Clone, Copy, Default)]
struct Node {
    end: bool,
    private: bool,
}

/// tldextract's suffix `Trie`, as a map from each node's dotted label path (`"co.uk"`) to its flags.
#[derive(Default)]
pub struct Suffixes(FxHashMap<String, Node>);

impl Suffixes {
    /// `Trie.create(public, private)`: public suffixes first, so a suffix in both lists ends up private.
    pub fn new(public: &[String], private: &[String]) -> Self {
        let mut nodes = FxHashMap::default();
        for (list, is_private) in [(public, false), (private, true)] {
            for suffix in list {
                let mut start = suffix.rfind('.').map_or(0, |i| i + 1);
                loop {
                    let key = &suffix[start..];
                    if !nodes.contains_key(key) {
                        nodes.insert(key.to_owned(), Node::default());
                    }
                    if start == 0 {
                        break;
                    }
                    start = suffix[..start - 1].rfind('.').map_or(0, |i| i + 1);
                }
                nodes.insert(suffix.clone(), Node { end: true, private: is_private });
            }
        }
        Self(nodes)
    }

    /// `(registered_domain, fqdn)` of `tldextract(url)`, or None when the host isn't plain ASCII, is bracketed
    /// (IPv6), or has an examined `xn--` label (IDNA decoding).
    pub fn split(&self, url: &str) -> Option<(String, String)> {
        let host = lenient_host(url)?;
        let labels: Vec<&str> = host.split('.').collect();
        let n = labels.len();
        let (mut suffix_idx, mut label_idx, mut private) = (n, n, false);
        let mut path: Option<String> = None;
        let child = |path: &Option<String>, label: &str| match path {
            None => label.to_owned(),
            Some(p) => format!("{label}.{p}"),
        };
        for label in labels.iter().rev() {
            let lower = label.to_ascii_lowercase();
            if lower.starts_with("xn--") {
                return None;
            }
            let key = child(&path, &lower);
            if let Some(node) = self.0.get(&key) {
                label_idx -= 1;
                if node.end {
                    (suffix_idx, private) = (label_idx, node.private);
                }
                path = Some(key);
                continue;
            }
            if let Some(wild) = self.0.get(&child(&path, "*")) {
                let exception = self.0.contains_key(&child(&path, &format!("!{lower}")));
                let idx = if exception { label_idx } else { label_idx - 1 };
                return Some(result(&labels, idx, wild.private));
            }
            break;
        }
        if suffix_idx == n {
            return Some((String::new(), String::new()));
        }
        Some(result(&labels, suffix_idx, private))
    }
}

/// `top_domain_under_public_suffix` and `fqdn` of the ExtractResult for public suffix index `idx`.
fn result(labels: &[&str], idx: usize, private: bool) -> (String, String) {
    let subdomain = if idx >= 2 { labels[..idx - 1].join(".") } else { String::new() };
    let domain = if idx > 0 { labels[idx - 1] } else { "" };
    let suffix = labels[idx..].join(".");
    if suffix.is_empty() {
        return (String::new(), String::new());
    }
    let registered = if domain.is_empty() { String::new() } else { format!("{domain}.{suffix}") };
    let fqdn = if domain.is_empty() && !private {
        String::new()
    } else {
        [subdomain.as_str(), domain, &suffix].iter().filter(|s| !s.is_empty()).copied().collect::<Vec<_>>().join(".")
    };
    (registered, fqdn)
}

/// `remote.lenient_netloc` followed by `_extract_netloc`'s ideographic dot replacement, for ASCII hosts only.
fn lenient_host(url: &str) -> Option<&str> {
    let authority = schemeless(url);
    let authority = authority.split('/').next().unwrap();
    let authority = authority.split('?').next().unwrap();
    let authority = authority.split('#').next().unwrap();
    let after_userinfo = authority.rsplit('@').next().unwrap();
    if after_userinfo.starts_with('[') {
        return None;
    }
    let host = strip(after_userinfo.split(':').next().unwrap());
    let host = host.trim_end_matches(['.', '\u{3002}', '\u{ff0e}', '\u{ff61}']);
    host.is_ascii().then_some(host)
}

fn schemeless(url: &str) -> &str {
    let is_scheme_char = |c: char| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.');
    match url.find("//") {
        Some(0) => &url[2..],
        Some(i) if i >= 2 && url[..i].ends_with(':') && url[..i - 1].chars().all(is_scheme_char) => &url[i + 2..],
        _ => url,
    }
}
