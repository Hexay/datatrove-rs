//! `BadWords`: C4BadWordsFilter's per-language regex search.

use datatrove_core::filters::badwords::BadWords;
use pyo3::prelude::*;

#[pyclass(name = "BadWords")]
pub struct PyBadWords(BadWords);

#[pymethods]
impl PyBadWords {
    /// `bounded`: the `(?:\W|^)(...)(?:\W|$)` form (every language but ja/th/zh).
    #[new]
    fn new(words: Vec<String>, bounded: bool) -> Self {
        Self(BadWords::new(&words, bounded))
    }

    /// Whether the regex finds a match in `text` (already lowercased).
    fn search(&self, py: Python<'_>, text: &str) -> bool {
        py.detach(|| self.0.search(text))
    }
}
