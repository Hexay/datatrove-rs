//! `BloomFilter` (SingleBloomFilter), `NGramsDecont` (NGramsDecontFilter/Indexer) and `UnigramLogProb`.

use crate::minhash::{shingler, u64s};
use crate::{cjk, words};
use datatrove_core::bloom::Bloom;
use datatrove_core::decont::Decont;
use datatrove_core::filters::unigram::Unigram;
use pyo3::prelude::*;
use pyo3::types::PyByteArray;

type Norm = (bool, bool, bool, bool, bool, bool, bool);

#[pyclass(name = "BloomFilter")]
pub struct PyBloom(Bloom);

#[pymethods]
impl PyBloom {
    /// `a`/`b` = the numpy uint64 `parameters`' bytes.
    #[new]
    #[allow(clippy::too_many_arguments)]
    fn new(
        n_grams: usize,
        norm: Norm,
        hash_fc: &str,
        precision: u32,
        a: &[u8],
        b: &[u8],
        m_bytes: u64,
        language: &str,
    ) -> PyResult<Self> {
        let s = shingler(n_grams, norm, hash_fc, precision, language)?;
        Ok(Self(Bloom::new(s, u64s(a)?, u64s(b)?, m_bytes)))
    }

    /// `(shingles, duplicate shingles)`, setting the bits in `bits`; None (bits untouched) when an index would fall
    /// outside `bits`.
    fn step(&mut self, text: &str, bits: &Bound<'_, PyByteArray>) -> PyResult<Option<(usize, usize)>> {
        let n = self.0.shingle(text);
        cjk::check()?;
        // SAFETY: no Python code runs while the slice is borrowed.
        let dups = self.0.update(unsafe { bits.as_bytes_mut() });
        Ok(dups.ok().map(|d| (n, d)))
    }
}

#[pyclass(name = "NGramsDecont")]
pub struct PyDecont(Decont);

#[pymethods]
impl PyDecont {
    #[new]
    fn new(n_grams: usize, norm: Norm, hash_fc: &str, precision: u32, language: &str) -> PyResult<Self> {
        Ok(Self(Decont::new(shingler(n_grams, norm, hash_fc, precision, language)?)))
    }

    /// Replaces the index with the little-endian uint64 hashes in `hashes`.
    fn set_index(&mut self, hashes: &[u8]) -> PyResult<()> {
        self.0.set_index(u64s(hashes)?);
        Ok(())
    }

    /// `(n_gram, hash)` of the first n-gram of `text` in the index.
    fn find(&mut self, text: &str) -> PyResult<Option<(String, u64)>> {
        let found = self.0.find(text);
        cjk::check()?;
        Ok(found)
    }

    /// `NGramsDecontIndexer.compute_hashes`.
    fn compute_hashes(
        &mut self,
        label: &str,
        query: Option<&str>,
        find_query: bool,
        find_overlap: bool,
    ) -> PyResult<Vec<u64>> {
        let hashes = self.0.compute_hashes(label, query, find_query, find_overlap);
        cjk::check()?;
        Ok(hashes)
    }
}

#[pyclass(name = "UnigramLogProb")]
pub struct PyUnigram(Unigram);

#[pymethods]
impl PyUnigram {
    /// `logs[i]` = numpy's log of `unigram_frequencies[words[i]]`; `missing` = numpy's `log(1e-9)`.
    #[new]
    fn new(words_: Vec<String>, logs: Vec<f64>, missing: f64, language: &str) -> PyResult<Self> {
        Ok(Self(Unigram::new(words_, logs, missing, words(language)?)))
    }

    /// The mean log-probability, or None for a doc without words.
    fn mean_logprob(&mut self, text: &str) -> PyResult<Option<f64>> {
        let mean = self.0.mean_logprob(text);
        cjk::check()?;
        Ok(mean)
    }
}

pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyBloom>()?;
    m.add_class::<PyDecont>()?;
    m.add_class::<PyUnigram>()
}
