//! `MinHasher`: MinhashDedupSignature's shingling and signatures.

use crate::{norm_config, words};
use datatrove_core::minhash::{signature, HashConfig, HashFn, MinHasher};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyBytes;

#[pyclass(name = "MinHasher")]
pub struct PyMinHasher {
    inner: MinHasher,
    a: Vec<u64>,
    b: Vec<u64>,
    precision: u32,
    shingles: Vec<u64>,
    sig: Vec<u64>,
}

pub(crate) fn u64s(bytes: &[u8]) -> PyResult<Vec<u64>> {
    let (chunks, rest) = bytes.as_chunks::<8>();
    if !rest.is_empty() {
        return Err(PyValueError::new_err("expected a little-endian uint64 buffer"));
    }
    Ok(chunks.iter().map(|&c| u64::from_le_bytes(c)).collect())
}

/// `n_grams`, `norm` (TextNormConfig fields in declaration order), HashConfig fields, spaCy code or tokenizer name.
pub(crate) fn shingler(
    n_grams: usize,
    norm: (bool, bool, bool, bool, bool, bool, bool),
    hash_fc: &str,
    precision: u32,
    language: &str,
) -> PyResult<MinHasher> {
    let hash_fn = match hash_fc {
        "xxhash" => HashFn::Xxhash,
        "sha1" => HashFn::Sha1,
        other => return Err(PyValueError::new_err(format!("Unknown hash_fc={other:?}"))),
    };
    Ok(MinHasher::new(n_grams, norm_config(norm), HashConfig { hash_fn, precision }, words(language)?))
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
        language: &str,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: shingler(n_grams, norm, hash_fc, precision, language)?,
            a: u64s(a)?,
            b: u64s(b)?,
            precision,
            shingles: Vec::new(),
            sig: Vec::new(),
        })
    }

    /// Little-endian uint64 bytes of the shingle hashes.
    fn shingles<'py>(&mut self, py: Python<'py>, text: &str) -> PyResult<Bound<'py, PyBytes>> {
        self.inner.shingles(text, &mut self.shingles);
        crate::cjk::check()?;
        Ok(PyBytes::new(py, &le_bytes(&self.shingles)))
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
