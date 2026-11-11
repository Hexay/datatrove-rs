//! `UrlLists`: URLFilter's blocklists, word checks and (optionally) tldextract's host split.

use datatrove_core::filters::strset::StrSet;
use datatrove_core::filters::tld::Suffixes;
use datatrove_core::filters::url::UrlLists;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::IntoPyObjectExt;

fn load(path: &str, extra: &[String]) -> PyResult<StrSet> {
    let bytes = std::fs::read(path).map_err(|e| PyValueError::new_err(format!("{path}: {e}")))?;
    let text = String::from_utf8(bytes).map_err(|e| PyValueError::new_err(format!("{path}: {e}")))?;
    Ok(StrSet::from_list_file(text, extra))
}

/// `True` to keep, else the exclusion reason.
fn verdict(py: Python<'_>, reason: Option<&'static str>) -> PyResult<Py<PyAny>> {
    match reason {
        None => true.into_py_any(py),
        Some(r) => r.into_py_any(py),
    }
}

#[pyclass(name = "UrlLists", frozen)]
pub struct PyUrlLists(UrlLists);

#[pymethods]
impl PyUrlLists {
    /// `domains_path`/`urls_path` are read as UTF-8 text like datatrove's `get_list`; the word lists are already
    /// normalized Python sets. `suffixes` = (public + extra, private-or-empty) tldextract suffixes, or None to leave
    /// host splitting to Python. ValueError when a file can't be read or isn't UTF-8.
    #[new]
    #[allow(clippy::too_many_arguments)]
    fn new(
        py: Python<'_>,
        domains_path: &str,
        urls_path: &str,
        extra_domains: Vec<String>,
        extra_urls: Vec<String>,
        banned_words: Vec<String>,
        soft_banned_words: Vec<String>,
        banned_subwords: Vec<String>,
        soft_word_threshold: f64,
        suffixes: Option<(Vec<String>, Vec<String>)>,
    ) -> PyResult<Self> {
        py.detach(|| {
            Ok(Self(UrlLists {
                domains: load(domains_path, &extra_domains)?,
                urls: load(urls_path, &extra_urls)?,
                banned_words: banned_words.into_iter().collect(),
                soft_banned_words: soft_banned_words.into_iter().collect(),
                subwords: UrlLists::subword_matcher(&banned_subwords),
                soft_word_threshold,
                suffixes: suffixes.map(|(public, private)| Suffixes::new(&public, &private)),
            }))
        })
    }

    /// The whole check: `True`, a reason, or None when the host needs Python's tldextract (then call `check`).
    fn check_url(&self, py: Python<'_>, url: &str) -> PyResult<Option<Py<PyAny>>> {
        self.0.check_url(url).map(|r| verdict(py, r)).transpose()
    }

    /// The checks after tldextract: `True` or the exclusion reason.
    fn check(&self, py: Python<'_>, url: &str, registered_domain: &str, fqdn: &str) -> PyResult<Py<PyAny>> {
        verdict(py, self.0.check(url, registered_domain, fqdn))
    }

    /// `(registered_domain, fqdn)` from the Rust host split, or None; for tests.
    fn split_host(&self, url: &str) -> Option<(String, String)> {
        self.0.suffixes.as_ref()?.split(url)
    }

    fn __len__(&self) -> usize {
        self.0.domains.len()
    }
}
