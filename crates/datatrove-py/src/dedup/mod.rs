//! Registers the dedup step bindings: MinHash signatures and clustering, sentence dedup.

mod sentdedup;

use pyo3::prelude::*;

pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<crate::minhash::PyMinHasher>()?;
    m.add_class::<crate::cluster::PyMinhashCluster>()?;
    sentdedup::register(m)
}
