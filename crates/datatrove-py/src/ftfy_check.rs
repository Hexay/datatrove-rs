//! `FtfyCheck`: FTFYFormatter's unchanged-text pre-check.

use datatrove_core::ftfy_check::FtfyCheck;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

#[pyclass(name = "FtfyCheck")]
pub struct PyFtfyCheck(FtfyCheck);

#[pymethods]
impl PyFtfyCheck {
    /// `forbidden`: inclusive code point ranges; `patterns`: Rust regexes that must not match.
    #[new]
    fn new(forbidden: Vec<(u32, u32)>, patterns: Vec<String>) -> PyResult<Self> {
        FtfyCheck::new(&forbidden, &patterns).map(Self).map_err(|e| PyValueError::new_err(e.to_string()))
    }

    /// None if ftfy cannot change `text`, else which check fired (0 = forbidden char, i + 1 = pattern i).
    fn first_hit(&self, py: Python<'_>, text: &str) -> Option<usize> {
        py.detach(|| self.0.first_hit(text))
    }
}

/// `re`'s `\s`, `\w` and `\d` on str patterns, as inclusive code point ranges from the embedded Python tables.
#[pyfunction]
pub fn re_category_ranges() -> [Vec<(u32, u32)>; 3] {
    datatrove_core::pyunicode::py().re_ranges.clone()
}
