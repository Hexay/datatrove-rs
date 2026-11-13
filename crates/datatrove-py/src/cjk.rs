//! Registers the Chinese/Japanese tokenizer data from the installed jieba and SudachiPy (`datatrove_rs/cjk.py`).

use cjk_tokenizer_rs::japanese::{Sudachi, SUDACHI_VERSION};
use cjk_tokenizer_rs::jieba::{Hmm, Jieba};
use datatrove_core::cjk_words;
use pyo3::exceptions::{PyException, PyValueError};
use pyo3::prelude::*;
use std::path::PathBuf;

pyo3::create_exception!(
    _core,
    TokenizerFallback,
    PyException,
    "datatrove's tokenizer raises on this text; run datatrove's Python code for the doc instead."
);

/// Err(TokenizerFallback) when a tokenization since the last check failed (`cjk_words::take_failure`).
pub(crate) fn check() -> PyResult<()> {
    if cjk_words::take_failure() {
        return Err(TokenizerFallback::new_err("tokenizer would raise"));
    }
    Ok(())
}

type Table = Vec<(String, Vec<(String, f64)>)>;

/// jieba's live `dt.FREQ` items and `dt.total`, and `finalseg`'s `start_P`, `trans_P`, `emit_P` (as item lists)
/// and `Force_Split_Words`.
#[pyfunction]
pub(crate) fn load_jieba(
    freq: Vec<(String, u64)>,
    total: u64,
    start: Vec<(String, f64)>,
    trans: Table,
    emit: Table,
    force_split: Vec<String>,
) -> PyResult<()> {
    let hmm = Hmm::new(start, trans, emit, force_split).map_err(PyValueError::new_err)?;
    cjk_words::load_jieba(Jieba::new(freq, total, hmm).map_err(PyValueError::new_err)?);
    Ok(())
}

/// The paths `sudachipy.Dictionary()` resolves: default settings file, resource dir, system dictionary.
#[pyfunction]
pub(crate) fn load_sudachi(py: Python<'_>, settings: PathBuf, resource_dir: PathBuf, dict: PathBuf) -> PyResult<()> {
    let sudachi = py.detach(|| Sudachi::new(&settings, &resource_dir, &dict)).map_err(PyValueError::new_err)?;
    cjk_words::load_sudachi(sudachi);
    Ok(())
}

/// The SudachiPy release whose analyzer is compiled in.
#[pyfunction]
pub(crate) fn sudachi_version() -> &'static str {
    SUDACHI_VERSION
}
