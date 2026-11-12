//! `MinhashCluster`: MinhashDedupCluster's union-find and output pass; Python does the file IO.

use datatrove_core::cluster::{Outputs, UnionFind};
use pyo3::exceptions::{PyKeyError, PyRuntimeError};
use pyo3::prelude::*;
use pyo3::types::PyBytes;

#[pyclass(name = "MinhashCluster")]
pub struct PyMinhashCluster {
    uf: Option<UnionFind>,
    out: Option<Outputs>,
}

type MetricTuple = (&'static str, Option<&'static str>, u64, u64, f64, u64, u64, f64);
type FileTuple<'py> = (u32, Bound<'py, PyBytes>, Bound<'py, PyBytes>, Bound<'py, PyBytes>);

#[pymethods]
impl PyMinhashCluster {
    #[new]
    fn new(ignore_index_matches: bool) -> Self {
        Self { uf: Some(UnionFind::new(ignore_index_matches)), out: None }
    }

    /// The next bytes of the current `.dups` file.
    fn feed(&mut self, py: Python<'_>, data: &[u8]) -> PyResult<()> {
        let uf = self.uf.as_mut().ok_or_else(finished)?;
        py.detach(|| uf.feed(data));
        Ok(())
    }

    /// Raises `struct.error` when the file ended mid-record, as `struct.iter_unpack` does.
    fn end_file(&mut self, py: Python<'_>) -> PyResult<()> {
        let uf = self.uf.as_mut().ok_or_else(finished)?;
        if uf.end_file().is_err() {
            let err = py.import("struct")?.getattr("error")?;
            return Err(PyErr::from_value(err.call1(("unpack requires a buffer of a multiple of 16 bytes",))?));
        }
        Ok(())
    }

    fn __len__(&self) -> usize {
        self.uf.as_ref().map_or(0, UnionFind::len)
    }

    fn finish(&mut self, py: Python<'_>, save_cluster_id: bool, save_cluster_size: bool) -> PyResult<()> {
        let uf = self.uf.take().ok_or_else(finished)?;
        self.out = Some(py.detach(|| uf.finish(save_cluster_id, save_cluster_size)));
        Ok(())
    }

    /// `(file, remove, clusters, sizes)` for the next file in ascending order, or None when done.
    fn next_file<'py>(&mut self, py: Python<'py>) -> PyResult<Option<FileTuple<'py>>> {
        let out = self.out.as_mut().ok_or_else(|| PyRuntimeError::new_err("call finish() first"))?;
        match py.detach(|| out.next_file()) {
            Err(missing) => Err(PyKeyError::new_err(((missing.0 >> 32) as u32, missing.0 as u32))),
            Ok(None) => Ok(None),
            Ok(Some(f)) => Ok(Some((
                f.file,
                PyBytes::new(py, &f.remove),
                PyBytes::new(py, &f.clusters),
                PyBytes::new(py, &f.sizes),
            ))),
        }
    }

    /// `(label, unit, total, n, mean, min, max, running_variance)` per stat, in datatrove's stats order.
    fn metrics(&self) -> Vec<MetricTuple> {
        let Some(out) = &self.out else { return Vec::new() };
        out.metrics()
            .into_iter()
            .map(|m| (m.label, m.unit, m.total, m.n, m.mean, m.min, m.max, m.running_variance))
            .collect()
    }
}

fn finished() -> PyErr {
    PyRuntimeError::new_err("finish() was already called")
}
