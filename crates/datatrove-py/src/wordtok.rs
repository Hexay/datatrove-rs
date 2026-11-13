//! The `wordtok` ports' names and test hooks.

use crate::words;
use datatrove_core::words::SentenceCount;
use pyo3::prelude::*;

/// Names of the other ported tokenizers (`indic:<code>`, `thai`, `vi`).
#[pyfunction]
pub fn word_tokenizers() -> Vec<String> {
    datatrove_core::words::word_tokenizers().collect()
}

/// `len(split_into_sentences(text, language))` as Rust sees it: an int, `"at_least_one"` or `"unknown"`.
#[pyfunction]
pub fn sentence_count(py: Python<'_>, text: &str, language: &str) -> PyResult<Py<PyAny>> {
    Ok(match words(language)?.sentence_count(text) {
        SentenceCount::Exact(n) => n.into_pyobject(py)?.into_any().unbind(),
        SentenceCount::AtLeastOne => "at_least_one".into_pyobject(py)?.into_any().unbind(),
        SentenceCount::Unknown => "unknown".into_pyobject(py)?.into_any().unbind(),
    })
}
