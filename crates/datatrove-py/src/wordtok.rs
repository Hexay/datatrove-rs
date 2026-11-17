//! The `wordtok` ports' names, data loading and test hooks.

use crate::words;
use datatrove_core::words::SentenceCount;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

/// Names of the other ported tokenizers (`indic:<code>`, `thai`, `vi`).
#[pyfunction]
fn word_tokenizers() -> Vec<String> {
    datatrove_core::words::word_tokenizers().collect()
}

/// `len(split_into_sentences(text, language))` as Rust sees it: an int, `"at_least_one"` or `"unknown"`.
#[pyfunction]
fn sentence_count(py: Python<'_>, text: &str, language: &str) -> PyResult<Py<PyAny>> {
    Ok(match words(language)?.sentence_count(text) {
        SentenceCount::Exact(n) => n.into_pyobject(py)?.into_any().unbind(),
        SentenceCount::AtLeastOne => "at_least_one".into_pyobject(py)?.into_any().unbind(),
        SentenceCount::Unknown => "unknown".into_pyobject(py)?.into_any().unbind(),
    })
}

/// `sent_tokenize(text)` of `language`'s tokenizer; None where Rust has no sentence splitter for it.
#[pyfunction]
fn sent_tokenize(text: &str, language: &str) -> PyResult<Option<Vec<String>>> {
    Ok(words(language)?.sentence_texts(text).map(|s| s.into_iter().map(|s| s.into_owned()).collect()))
}

/// Gives the Thai port pythainlp's crfcut sentence splitter: its model file and word sets.
#[pyfunction]
fn load_thai_crfcut(model_path: &str, enders: Vec<String>, starters: Vec<String>) -> PyResult<()> {
    datatrove_core::wordtok::load_thai_crfcut(model_path, enders, starters).map_err(PyValueError::new_err)
}

/// Gives the Urdu port urduhack's `eos._URDU_CONJUNCTIONS` and `eos._URDU_NEWLINE_WORDS`.
#[pyfunction]
fn load_urdu_eos(conjunctions: Vec<String>, newline_words: Vec<String>) {
    datatrove_core::wordtok::load_urdu_eos(conjunctions, newline_words)
}

pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(word_tokenizers, m)?)?;
    m.add_function(wrap_pyfunction!(sentence_count, m)?)?;
    m.add_function(wrap_pyfunction!(sent_tokenize, m)?)?;
    m.add_function(wrap_pyfunction!(load_thai_crfcut, m)?)?;
    m.add_function(wrap_pyfunction!(load_urdu_eos, m)?)
}
