//! datatrove's stats steps (`datatrove.pipeline.stats`): each step's `extract_stats` (`extract`) and
//! `BaseStats.run`'s per-group bookkeeping (`acc`), with CPython's float arithmetic and formatting (`pyfloat`).

mod acc;
mod extract;
mod pyfloat;

pub use acc::{Accumulator, DocKeys, Group, GroupKind, Row, Table, Total};
pub use extract::{DocStats, Extractor, LineStats, ParagraphStats, SentenceStats, WordStats};
pub use pyfloat::{histogram_key, py_repr, py_round};

/// A Python `int` with |v| <= 2**53 (so it converts to float exactly) or a `float`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Num {
    Int(i64),
    Float(f64),
}

impl Num {
    pub const MAX_EXACT_INT: i64 = 1 << 53;

    #[inline]
    pub fn f(self) -> f64 {
        match self {
            Num::Int(v) => v as f64,
            Num::Float(v) => v,
        }
    }

    /// Python `self > other`.
    #[inline]
    pub fn gt(self, other: Num) -> bool {
        match (self, other) {
            (Num::Int(a), Num::Int(b)) => a > b,
            _ => self.f() > other.f(),
        }
    }
}
