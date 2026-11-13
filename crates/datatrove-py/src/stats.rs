//! The stats steps (`datatrove_rs/stats.py`): `extract_stats` per step and `BaseStats.run`'s bookkeeping.

use crate::{cjk, words};
use datatrove_core::stats::{
    Accumulator, DocKeys, DocStats, Extractor, GroupKind, LineStats, Num, ParagraphStats, SentenceStats, Total,
    WordStats,
};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyFloat, PyInt, PyString, PyTuple};
use pyo3::IntoPyObjectExt;

fn num_to_py(py: Python<'_>, n: Num) -> PyResult<Py<PyAny>> {
    match n {
        Num::Int(v) => v.into_py_any(py),
        Num::Float(v) => v.into_py_any(py),
    }
}

/// `v` as a `Num` when it is exactly an int Rust keeps exact or a finite float; None for anything else (bool,
/// big ints, nan, numpy scalars, ...), which the caller leaves to datatrove's code.
fn num(v: &Bound<'_, PyAny>) -> Option<Num> {
    if v.is_exact_instance_of::<PyInt>() {
        let i: i64 = v.extract().ok()?;
        return (i.abs() <= Num::MAX_EXACT_INT).then_some(Num::Int(i));
    }
    if v.is_exact_instance_of::<PyFloat>() {
        let f: f64 = v.extract().ok()?;
        return f.is_finite().then_some(Num::Float(f));
    }
    None
}

fn exact_str<'py>(v: Option<Bound<'py, PyAny>>) -> Option<Bound<'py, PyString>> {
    v.filter(|v| v.is_exact_instance_of::<PyString>()).and_then(|v| v.cast_into::<PyString>().ok())
}

fn opt_str<'a>(s: &'a Option<Bound<'_, PyString>>) -> PyResult<Option<&'a str>> {
    s.as_ref().map(|s| s.to_str()).transpose()
}

#[pyclass(name = "StatsExtractor")]
pub(crate) struct PyStatsExtractor {
    inner: Extractor,
    out: Vec<Num>,
}

impl PyStatsExtractor {
    fn of(inner: Extractor) -> Self {
        Self { inner, out: Vec::new() }
    }
}

#[pymethods]
impl PyStatsExtractor {
    #[staticmethod]
    fn words(language: &str, short: Vec<i64>, long: Vec<i64>, stop_words: Vec<String>) -> PyResult<Self> {
        let stop_words = stop_words.into_iter().collect();
        Ok(Self::of(Extractor::Word(WordStats { words: words(language)?, short, long, stop_words })))
    }

    #[staticmethod]
    fn sentences(language: &str, short: Vec<i64>, long: Vec<i64>) -> PyResult<Self> {
        Ok(Self::of(Extractor::Sentence(SentenceStats { words: words(language)?, short, long })))
    }

    #[staticmethod]
    fn doc() -> Self {
        Self::of(Extractor::Doc(DocStats))
    }

    #[staticmethod]
    fn lines(short: Vec<i64>, long: Vec<i64>, ignore_empty_lines: bool) -> Self {
        Self::of(Extractor::Line(LineStats { short, long, ignore_empty_lines }))
    }

    #[staticmethod]
    fn paragraphs(short: Vec<i64>, long: Vec<i64>) -> Self {
        Self::of(Extractor::Paragraph(ParagraphStats { short, long }))
    }

    /// The `extract_stats` values in its dict order, or None where datatrove's code must run.
    fn extract<'py>(&mut self, py: Python<'py>, text: &str) -> PyResult<Option<Bound<'py, PyTuple>>> {
        let ok = self.inner.extract(text, &mut self.out);
        cjk::check()?;
        if !ok {
            return Ok(None);
        }
        let values = self.out.iter().map(|&n| num_to_py(py, n)).collect::<PyResult<Vec<_>>>()?;
        PyTuple::new(py, values).map(Some)
    }
}

type Row<'a> = (&'a str, Py<PyAny>, u64, f64, Py<PyAny>, Py<PyAny>, f64);

#[pyclass(name = "StatsAccumulator")]
pub(crate) struct PyStatsAccumulator {
    acc: Accumulator,
    kinds: Vec<GroupKind>,
    digits: usize,
}

#[pymethods]
impl PyStatsAccumulator {
    /// `groups`: `BaseStats.groups` without duplicates; `digits`: `histogram_round_digits`.
    #[new]
    fn new(groups: Vec<String>, digits: usize) -> PyResult<Self> {
        let kinds = groups
            .iter()
            .map(|g| GroupKind::parse(g).ok_or_else(|| PyValueError::new_err(format!("unknown group {g:?}"))))
            .collect::<PyResult<Vec<_>>>()?;
        Ok(Self { acc: Accumulator::new(&kinds, digits), kinds, digits })
    }

    /// `BaseStats.run`'s group loop for one doc, after `get_kv` set `metadata["fqdn"/"suffix"]` for those groups.
    /// False, recording nothing, when a stat, key or token count is not one Rust keeps exactly; the caller then
    /// continues in Python from `drain()`.
    fn add(&mut self, doc_stats: &Bound<'_, PyDict>, metadata: &Bound<'_, PyAny>, chars: i64) -> PyResult<bool> {
        let Ok(meta) = metadata.cast::<PyDict>() else { return Ok(false) };
        let mut items = Vec::with_capacity(doc_stats.len());
        for (k, v) in doc_stats.iter() {
            let (Some(k), Some(v)) = (exact_str(Some(k)), num(&v)) else { return Ok(false) };
            items.push((k, v));
        }
        // a missing or non-str key for a group that needs it makes `accepts` false below
        let key = |kind, name| -> PyResult<_> {
            Ok(if self.kinds.contains(&kind) { exact_str(meta.get_item(name)?) } else { None })
        };
        let (fqdn, suffix) = (key(GroupKind::Fqdn, "fqdn")?, key(GroupKind::Suffix, "suffix")?);
        let tokens = match meta.get_item("token_count")? {
            None => None,
            Some(v) => match num(&v) {
                Some(n) => Some(n),
                None => return Ok(false),
            },
        };
        let (Ok(fqdn), Ok(suffix)) = (opt_str(&fqdn), opt_str(&suffix)) else { return Ok(false) };
        let mut stats = Vec::with_capacity(items.len());
        for (k, v) in &items {
            let Ok(k) = k.to_str() else { return Ok(false) };
            stats.push((k, *v));
        }
        let doc = DocKeys { fqdn, suffix, chars, tokens };
        if !self.acc.accepts(&doc) {
            return Ok(false);
        }
        self.acc.add(&stats, &doc);
        Ok(true)
    }

    /// Every group's tables as `[(group, [(stat, [(key, total, n, mean, min, max, running_variance)])])]` in
    /// insertion order, leaving the accumulator empty.
    fn drain(&mut self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let acc = std::mem::replace(&mut self.acc, Accumulator::new(&self.kinds, self.digits));
        let mut out = Vec::with_capacity(acc.groups.len());
        for g in &acc.groups {
            let mut tables = Vec::with_capacity(g.tables.len());
            for t in &g.tables {
                let rows = t.rows.iter().map(|(key, r)| -> PyResult<Row<'_>> {
                    let total = match r.total {
                        Total::Int(v) => v.into_py_any(py)?,
                        Total::Float(v) => v.into_py_any(py)?,
                    };
                    let (min, max) = (num_to_py(py, r.min)?, num_to_py(py, r.max)?);
                    Ok((key, total, r.n, r.mean, min, max, r.running_variance))
                });
                tables.push((t.name.as_str(), rows.collect::<PyResult<Vec<_>>>()?));
            }
            out.push((g.kind.name(), tables));
        }
        out.into_py_any(py)
    }
}

/// `str(round(value, digits))` as the histogram group keys it (for tests).
#[pyfunction]
fn stats_histogram_key(value: &Bound<'_, PyAny>, digits: usize) -> PyResult<String> {
    let v = num(value).ok_or_else(|| PyValueError::new_err("not an exact int or finite float"))?;
    let mut s = String::new();
    datatrove_core::stats::histogram_key(v, digits, &mut s);
    Ok(s)
}

pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyStatsExtractor>()?;
    m.add_class::<PyStatsAccumulator>()?;
    m.add_function(wrap_pyfunction!(stats_histogram_key, m)?)
}
