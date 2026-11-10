//! PyO3 bindings; `python/datatrove_rs` wraps these in datatrove `PipelineStep` subclasses.

use datatrove_core::filters::c4::{C4Config, LineOutcome, C4};
use datatrove_core::filters::fineweb::{FineWeb, FineWebConfig};
use datatrove_core::filters::gopher_quality::{GopherQuality, GopherQualityConfig};
use datatrove_core::filters::gopher_repetition::{GopherRepetition, GopherRepetitionConfig};
use datatrove_core::filters::{FilterResult, Verdict, ZeroDivision};
use datatrove_core::minhash::{signature, HashConfig, HashFn, MinHasher};
use datatrove_core::pyunicode::py;
use datatrove_core::simplify::{NormConfig, Simplifier};
use datatrove_core::words::Words;
use pyo3::exceptions::{PyValueError, PyZeroDivisionError};
use pyo3::prelude::*;
use pyo3::types::PyBytes;
use pyo3::IntoPyObjectExt;

/// `True`, `(False, reason)`, or `ZeroDivisionError` — what datatrove's `filter()` would produce.
fn to_py(py: Python<'_>, r: FilterResult) -> PyResult<Py<PyAny>> {
    match r {
        Err(ZeroDivision) => Err(PyZeroDivisionError::new_err("division by zero")),
        Ok(Verdict::Keep) => true.into_py_any(py),
        Ok(Verdict::Drop(reason)) => (false, reason.as_ref()).into_py_any(py),
    }
}

#[pyclass(name = "GopherQuality")]
struct PyGopherQuality(GopherQuality);

#[pymethods]
impl PyGopherQuality {
    #[new]
    #[allow(clippy::too_many_arguments)]
    fn new(
        min_doc_words: Option<f64>,
        max_doc_words: Option<f64>,
        min_avg_word_length: Option<f64>,
        max_avg_word_length: Option<f64>,
        max_symbol_word_ratio: Option<f64>,
        max_bullet_lines_ratio: Option<f64>,
        max_ellipsis_lines_ratio: Option<f64>,
        max_non_alpha_words_ratio: Option<f64>,
        min_stop_words: Option<f64>,
        stop_words: Vec<String>,
    ) -> Self {
        Self(GopherQuality::new(GopherQualityConfig {
            min_doc_words,
            max_doc_words,
            min_avg_word_length,
            max_avg_word_length,
            max_symbol_word_ratio,
            max_bullet_lines_ratio,
            max_ellipsis_lines_ratio,
            max_non_alpha_words_ratio,
            min_stop_words,
            stop_words,
        }))
    }

    fn filter(&mut self, py: Python<'_>, text: &str) -> PyResult<Py<PyAny>> {
        to_py(py, self.0.filter(text))
    }
}

#[pyclass(name = "GopherRepetition")]
struct PyGopherRepetition(GopherRepetition);

#[pymethods]
impl PyGopherRepetition {
    #[new]
    fn new(
        dup_line_frac: Option<f64>,
        dup_para_frac: Option<f64>,
        dup_line_char_frac: Option<f64>,
        dup_para_char_frac: Option<f64>,
        top_n_grams: Vec<(usize, f64)>,
        dup_n_grams: Vec<(usize, f64)>,
    ) -> Self {
        Self(GopherRepetition::new(GopherRepetitionConfig {
            dup_line_frac,
            dup_para_frac,
            dup_line_char_frac,
            dup_para_char_frac,
            top_n_grams,
            dup_n_grams,
        }))
    }

    fn filter(&mut self, py: Python<'_>, text: &str) -> PyResult<Py<PyAny>> {
        to_py(py, self.0.filter(text))
    }
}

#[pyclass(name = "FineWebQuality")]
struct PyFineWebQuality(FineWeb);

#[pymethods]
impl PyFineWebQuality {
    #[new]
    fn new(
        line_punct_thr: f64,
        line_punct_exclude_zero: bool,
        stop_chars: Vec<String>,
        short_line_thr: f64,
        short_line_length: i64,
        char_duplicates_ratio: f64,
        new_line_ratio: f64,
    ) -> Self {
        Self(FineWeb::new(FineWebConfig {
            line_punct_thr,
            line_punct_exclude_zero,
            stop_chars,
            short_line_thr,
            short_line_length,
            char_duplicates_ratio,
            new_line_ratio,
        }))
    }

    fn filter(&mut self, py: Python<'_>, text: &str) -> PyResult<Py<PyAny>> {
        to_py(py, self.0.filter(text))
    }
}

#[pyclass(name = "C4Quality")]
struct PyC4Quality(C4);

#[pymethods]
impl PyC4Quality {
    #[new]
    #[allow(clippy::too_many_arguments)]
    fn new(
        split_paragraph: bool,
        remove_citations: bool,
        filter_no_terminal_punct: bool,
        min_num_sentences: i64,
        min_words_per_line: i64,
        max_word_length: i64,
        filter_lorem_ipsum: bool,
        filter_javascript: bool,
        filter_curly_bracket: bool,
        filter_policy: bool,
    ) -> Self {
        Self(C4::new(C4Config {
            split_paragraph,
            remove_citations,
            filter_no_terminal_punct,
            min_num_sentences,
            min_words_per_line,
            max_word_length,
            filter_lorem_ipsum,
            filter_javascript,
            filter_curly_bracket,
            filter_policy,
        }))
    }

    /// `(result, new_text | None, line_outcomes)`; outcome bytes index `LINE_STATS` in `filters.py`.
    fn filter<'py>(
        &mut self,
        py: Python<'py>,
        text: &str,
    ) -> PyResult<(Py<PyAny>, Option<String>, Bound<'py, PyBytes>)> {
        let out = self.0.filter(text);
        let codes: Vec<u8> = out.lines.iter().map(|o| line_code(*o)).collect();
        Ok((to_py(py, Ok(out.verdict))?, out.text, PyBytes::new(py, &codes)))
    }
}

fn line_code(o: LineOutcome) -> u8 {
    match o {
        LineOutcome::TooLongWord => 0,
        LineOutcome::NoTerminalPunc => 1,
        LineOutcome::TooFewWords => 2,
        LineOutcome::Javascript => 3,
        LineOutcome::Policy => 4,
        LineOutcome::Kept => 5,
        LineOutcome::DroppedDoc => 6,
    }
}

#[pyclass(name = "MinHasher")]
struct PyMinHasher {
    inner: MinHasher,
    a: Vec<u64>,
    b: Vec<u64>,
    precision: u32,
    shingles: Vec<u64>,
    sig: Vec<u64>,
}

fn u64s(bytes: &[u8]) -> PyResult<Vec<u64>> {
    let (chunks, rest) = bytes.as_chunks::<8>();
    if !rest.is_empty() {
        return Err(PyValueError::new_err("expected a little-endian uint64 buffer"));
    }
    Ok(chunks.iter().map(|&c| u64::from_le_bytes(c)).collect())
}

#[pymethods]
impl PyMinHasher {
    /// `norm` = TextNormConfig fields in declaration order; `a`/`b` = the numpy uint64 parameters' bytes.
    #[new]
    fn new(
        n_grams: usize,
        norm: (bool, bool, bool, bool, bool, bool, bool),
        hash_fc: &str,
        precision: u32,
        a: &[u8],
        b: &[u8],
    ) -> PyResult<Self> {
        let hash_fn = match hash_fc {
            "xxhash" => HashFn::Xxhash,
            "sha1" => HashFn::Sha1,
            other => return Err(PyValueError::new_err(format!("Unknown hash_fc={other:?}"))),
        };
        Ok(Self {
            inner: MinHasher::new(n_grams, norm_config(norm), HashConfig { hash_fn, precision }),
            a: u64s(a)?,
            b: u64s(b)?,
            precision,
            shingles: Vec::new(),
            sig: Vec::new(),
        })
    }

    /// Little-endian uint64 bytes of the shingle hashes.
    fn shingles<'py>(&mut self, py: Python<'py>, text: &str) -> Bound<'py, PyBytes> {
        self.inner.shingles(text, &mut self.shingles);
        PyBytes::new(py, &le_bytes(&self.shingles))
    }

    /// `get_signature`: `num_buckets` lists of hashes.
    fn signature(&mut self, shingles: &[u8], num_buckets: usize) -> PyResult<Vec<Vec<u64>>> {
        let s = u64s(shingles)?;
        signature(&s, &self.a, &self.b, self.precision, &mut self.sig);
        if num_buckets == 0 || !self.sig.len().is_multiple_of(num_buckets) {
            return Err(PyValueError::new_err("array split does not result in an equal division"));
        }
        Ok(self.sig.chunks(self.sig.len() / num_buckets).map(<[u64]>::to_vec).collect())
    }
}

fn le_bytes(v: &[u64]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

fn norm_config(norm: (bool, bool, bool, bool, bool, bool, bool)) -> NormConfig {
    let (
        lowercase,
        norm_whitespace,
        remove_punctuation,
        norm_unicode_diacritics,
        norm_numbers,
        norm_weekdays,
        norm_monthnames,
    ) = norm;
    NormConfig {
        lowercase,
        norm_whitespace,
        remove_punctuation,
        norm_unicode_diacritics,
        norm_numbers,
        norm_weekdays,
        norm_monthnames,
    }
}

/// `datatrove.utils.text.simplify_text`; `norm` = TextNormConfig fields in declaration order.
#[pyfunction]
fn simplify_text(text: &str, norm: (bool, bool, bool, bool, bool, bool, bool)) -> String {
    Simplifier::new(norm_config(norm)).simplify(text).to_owned()
}

/// `split_into_words(text, "eng")`.
#[pyfunction]
fn split_into_words(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    Words::default().words(text, &mut out);
    out.into_iter().map(str::to_owned).collect()
}

/// `split_into_sentences(text, "eng")`.
#[pyfunction]
fn split_into_sentences(text: &str) -> Vec<String> {
    Words::default().sentences(text).into_iter().map(str::to_owned).collect()
}

/// (python_version, unidata_version) the embedded Unicode tables were exported from.
#[pyfunction]
fn unicode_tables_version() -> (String, String) {
    (py().python_version.clone(), py().unidata_version.clone())
}

#[pymodule]
fn _core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyGopherQuality>()?;
    m.add_class::<PyGopherRepetition>()?;
    m.add_class::<PyFineWebQuality>()?;
    m.add_class::<PyC4Quality>()?;
    m.add_class::<PyMinHasher>()?;
    m.add_function(wrap_pyfunction!(unicode_tables_version, m)?)?;
    m.add_function(wrap_pyfunction!(simplify_text, m)?)?;
    m.add_function(wrap_pyfunction!(split_into_words, m)?)?;
    m.add_function(wrap_pyfunction!(split_into_sentences, m)?)?;
    Ok(())
}
