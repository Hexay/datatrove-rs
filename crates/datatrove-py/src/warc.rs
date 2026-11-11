//! `WarcRecords(fileobj)`: the selected records of a gzip WARC read from a binary Python file object.

use datatrove_core::warc::{gzip_records, GzipRecords, Item};
use pyo3::exceptions::PyUnicodeDecodeError;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyString};
use pyo3::IntoPyObjectExt;
use std::io::{self, Read};

/// `io.RawIOBase`-style reads from a Python file object; Python exceptions surface as io errors, which make the
/// record loop hand the file back to datatrove, so they are raised again there.
struct PyReader(Py<PyAny>);

impl Read for PyReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        Python::attach(|py| {
            let data = self.0.bind(py).call_method1("read", (buf.len(),)).map_err(io::Error::other)?;
            let bytes = data.cast::<PyBytes>().map_err(|e| io::Error::other(e.to_string()))?.as_bytes();
            let n = bytes.len().min(buf.len());
            buf[..n].copy_from_slice(&bytes[..n]);
            Ok(n)
        })
    }
}

/// Iterates `(index, text, id, url, date)` for documents, `(index, record_bytes)` for records datatrove's
/// `process_record` must handle, and a bare `index` when datatrove must read the rest of the file from there.
#[pyclass(name = "WarcRecords")]
pub struct PyWarcRecords(GzipRecords<PyReader>);

#[pymethods]
impl PyWarcRecords {
    #[new]
    fn new(fileobj: Py<PyAny>) -> Self {
        Self(gzip_records(PyReader(fileobj)))
    }

    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__(&mut self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        let item = match self.0.next_item() {
            Item::End => return Ok(None),
            Item::Fallback { index } => index.into_py_any(py)?,
            Item::Raw { index, raw } => (index, PyBytes::new(py, raw)).into_py_any(py)?,
            Item::Doc(doc) => match PyString::from_bytes(py, doc.body) {
                Ok(text) => (doc.index, text, doc.id, doc.url, doc.date).into_py_any(py)?,
                Err(e) if e.is_instance_of::<PyUnicodeDecodeError>(py) => {
                    (doc.index, PyBytes::new(py, doc.raw)).into_py_any(py)?
                }
                Err(e) => return Err(e),
            },
        };
        Ok(Some(item))
    }
}
