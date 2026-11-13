//! `SentDedupSigner` and `SentDedupFilter`: SentenceDedupSignature and SentenceDedupFilter's Rust halves.

use crate::{cjk, norm_config, words};
use datatrove_core::minhash::{HashConfig, HashFn};
use datatrove_core::sentdedup::{sorted_records, DupRemover, Sig, Signer};
use datatrove_core::words::SentenceCount;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyBytes;

pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PySigner>()?;
    m.add_class::<PyDupRemover>()
}

#[pyclass(name = "SentDedupSigner")]
struct PySigner {
    inner: Signer,
    precision: u32,
    sigs: Vec<Sig>,
    buf: Vec<(u64, u64)>,
}

#[pymethods]
impl PySigner {
    /// `norm` = TextNormConfig fields in declaration order.
    #[new]
    fn new(
        n_sentences: usize,
        split_sentences: bool,
        norm: (bool, bool, bool, bool, bool, bool, bool),
        hash_fc: &str,
        precision: u32,
        language: &str,
    ) -> PyResult<Self> {
        let hash_fn = match hash_fc {
            "xxhash" => HashFn::Xxhash,
            "sha1" => HashFn::Sha1,
            other => return Err(PyValueError::new_err(format!("Unknown hash_fc={other:?}"))),
        };
        let hash = HashConfig { hash_fn, precision };
        let inner = Signer::new(n_sentences, split_sentences, norm_config(norm), hash, words(language)?);
        Ok(Self { inner, precision, sigs: Vec::new(), buf: Vec::new() })
    }

    #[getter]
    fn supported(&self) -> bool {
        self.inner.supported()
    }

    /// `get_hashes` as (hash, sentence index) pairs; None where datatrove's code must run.
    fn hashes(&mut self, text: &str) -> PyResult<Option<Vec<(u64, u64)>>> {
        let ok = self.inner.hashes(text, &mut self.buf);
        cjk::check()?;
        Ok(ok.then(|| self.buf.clone()))
    }

    /// Appends doc `doc_idx`'s signatures; False (nothing appended) where datatrove's code must run.
    fn add(&mut self, text: &str, doc_idx: u64) -> PyResult<bool> {
        let ok = self.inner.hashes(text, &mut self.buf);
        cjk::check()?;
        if ok {
            self.sigs.extend(self.buf.iter().map(|&(h, s)| (h, doc_idx, s)));
        }
        Ok(ok)
    }

    /// Appends signature tuples datatrove's `get_hashes` produced.
    fn extend(&mut self, sigs: Vec<Sig>) {
        self.sigs.extend(sigs);
    }

    fn clear(&mut self) {
        self.sigs.clear();
    }

    /// Every signature, in the order added.
    fn signatures(&self) -> Vec<Sig> {
        self.sigs.clone()
    }

    /// The sorted packed records `save_hashes` writes (`sorted_records`, which sorts the signatures in place);
    /// None, leaving them in order, where a value overflows its field.
    fn records<'py>(&mut self, py: Python<'py>) -> Option<Bound<'py, PyBytes>> {
        sorted_records(&mut self.sigs, self.precision).map(|r| PyBytes::new(py, &r))
    }
}

#[pyclass(name = "SentDedupFilter")]
struct PyDupRemover(DupRemover);

#[pymethods]
impl PyDupRemover {
    #[new]
    fn new(n_sentences: i64, split_sentences: bool, min_words_to_remove_span: f64, language: &str) -> PyResult<Self> {
        Ok(Self(DupRemover::new(n_sentences, split_sentences, min_words_to_remove_span, words(language)?)))
    }

    #[getter]
    fn supported(&self) -> bool {
        self.0.supported()
    }

    /// `remove_dup_sentences(text, dups)` as (text, original_formatted, n_sentences, n_kept); `dups` is the
    /// little-endian uint16 bytes of the doc's duplicate sentence indices. None where datatrove's code must run.
    fn remove(&mut self, text: &str, dups: &[u8]) -> PyResult<Option<(String, String, usize, usize)>> {
        let (chunks, rest) = dups.as_chunks::<2>();
        if !rest.is_empty() {
            return Err(PyValueError::new_err("expected a little-endian uint16 buffer"));
        }
        let dups: Vec<u16> = chunks.iter().map(|&c| u16::from_le_bytes(c)).collect();
        let out = self.0.remove(text, &dups);
        cjk::check()?;
        Ok(out.map(|r| (r.text, r.original, r.sentences, r.kept)))
    }

    fn word_count(&mut self, text: &str) -> PyResult<usize> {
        let n = self.0.word_count(text);
        cjk::check()?;
        Ok(n)
    }

    /// `len(split_into_sentences(text))` as (n, exact); when not exact, n is only a lower bound.
    fn sentence_count(&mut self, text: &str) -> PyResult<(usize, bool)> {
        let n = self.0.sentence_count(text);
        cjk::check()?;
        Ok(match n {
            SentenceCount::Exact(n) => (n, true),
            SentenceCount::AtLeastOne => (1, false),
            SentenceCount::Unknown => (0, false),
        })
    }
}
