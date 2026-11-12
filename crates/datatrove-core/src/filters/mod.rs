//! Ports of datatrove's heuristic filters. Each returns what `BaseFilter.filter` returns in Python,
//! evaluating checks in the same order so the first failing reason (and any ZeroDivisionError) matches.

pub mod badwords;
pub mod c4;
pub mod fineweb;
pub mod gopher_quality;
pub mod gopher_repetition;
pub mod strset;
pub mod tld;
pub mod url;

use std::borrow::Cow;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Keep,
    Drop(Cow<'static, str>),
}

/// datatrove raises `ZeroDivisionError` on some degenerate docs (e.g. empty text in GopherRepetition).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZeroDivision;

pub type FilterResult = Result<Verdict, ZeroDivision>;

/// Python `a / b` on ints.
#[inline]
pub(crate) fn div(a: usize, b: usize) -> Result<f64, ZeroDivision> {
    if b == 0 {
        Err(ZeroDivision)
    } else {
        Ok(a as f64 / b as f64)
    }
}

/// Python truthiness of an optional numeric threshold: `None` and `0` disable a check.
#[inline]
pub(crate) fn on(x: Option<f64>) -> Option<f64> {
    x.filter(|&v| v != 0.0)
}

#[inline]
pub(crate) fn drop(reason: &'static str) -> FilterResult {
    Ok(Verdict::Drop(Cow::Borrowed(reason)))
}
