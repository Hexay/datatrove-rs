//! `FtfyCheck` (FTFYFormatter's unchanged-text pre-check) and `FtfyFixer` (the `ftfy.fix_text` port).

use datatrove_core::ftfy::{self, EncodingFlags, FixConfig, Unescape};
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

#[pyclass(name = "FtfyFixer", frozen)]
pub struct PyFtfyFixer(FixConfig);

#[pymethods]
impl PyFtfyFixer {
    /// `unescape_html`: 0 off, 1 on, 2 "auto"; `encoding`: the four flags `fix_encoding` reads, in
    /// `EncodingFlags` order; the rest are `TextFixerConfig` fields.
    #[new]
    fn new(
        unescape_html: u8,
        remove_terminal_escapes: bool,
        fix_encoding: bool,
        encoding: (bool, bool, bool, bool),
        fix_c1_controls: bool,
        remove_control_chars: bool,
        max_decode_length: usize,
    ) -> PyResult<Self> {
        let unescape_html = match unescape_html {
            0 => Unescape::Off,
            1 => Unescape::On,
            2 => Unescape::Auto,
            _ => return Err(PyValueError::new_err("unescape_html must be 0, 1 or 2")),
        };
        if max_decode_length == 0 {
            return Err(PyValueError::new_err("max_decode_length must be positive"));
        }
        let (restore_byte_a0, replace_lossy_sequences, decode_inconsistent_utf8, enc_c1) = encoding;
        let encoding = EncodingFlags {
            restore_byte_a0,
            replace_lossy_sequences,
            decode_inconsistent_utf8,
            fix_c1_controls: enc_c1,
        };
        ftfy::tables();
        Ok(Self(FixConfig {
            unescape_html,
            remove_terminal_escapes,
            fix_encoding,
            encoding,
            fix_c1_controls,
            remove_control_chars,
            max_decode_length,
        }))
    }

    /// `ftfy.fix_text(text, config)`, or None when that is `text` itself.
    fn fix(&self, py: Python<'_>, text: &str) -> Option<String> {
        py.detach(|| ftfy::fix_text(&self.0, text))
    }
}

/// `(ftfy version, Python version, Unicode version)` the embedded ftfy tables were exported from.
#[pyfunction]
fn ftfy_tables_version() -> (String, String, String) {
    let t = ftfy::tables();
    (t.ftfy_version.clone(), t.python_version.clone(), t.unidata_version.clone())
}

/// `re`'s `\s`, `\w` and `\d` on str patterns, as inclusive code point ranges from the embedded Python tables.
#[pyfunction]
fn re_category_ranges() -> [Vec<(u32, u32)>; 3] {
    datatrove_core::pyunicode::py().re_ranges.clone()
}

pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyFtfyCheck>()?;
    m.add_class::<PyFtfyFixer>()?;
    m.add_function(wrap_pyfunction!(ftfy_tables_version, m)?)?;
    m.add_function(wrap_pyfunction!(re_category_ranges, m)?)
}
