//! `Pii`: PIIFormatter's email and IP replacement.

use datatrove_core::pii::{IpRules, Pii, Replacements};
use pyo3::prelude::*;

#[pyclass(name = "Pii")]
pub struct PyPii {
    inner: Pii,
    emails: Vec<String>,
    ips: Vec<String>,
}

#[pymethods]
impl PyPii {
    /// `global_from`: sorted `(first address, is_global)` intervals built from the running Python's `ipaddress`.
    #[new]
    fn new(
        global_from: Vec<(u32, bool)>,
        reject_leading_zeros: bool,
        public_only: bool,
        emails: Vec<String>,
        ips: Vec<String>,
    ) -> Self {
        let rules = IpRules { global_from, reject_leading_zeros, public_only };
        Self { inner: Pii::new(rules), emails, ips }
    }

    /// `(new_text or None if unchanged, email counter, ip counter)`; counters are the replacers' `_replace_i`.
    /// Empty replacement lists must stay in Python (datatrove raises on the first match).
    fn format(
        &self,
        py: Python<'_>,
        text: &str,
        remove_emails: bool,
        remove_ips: bool,
        email_i: usize,
        ip_i: usize,
    ) -> (Option<String>, usize, usize) {
        py.detach(|| {
            let mut emails = Replacements { items: &self.emails, next: email_i };
            let mut ips = Replacements { items: &self.ips, next: ip_i };
            let after_emails = if remove_emails { self.inner.replace_emails(text, &mut emails) } else { None };
            let current = after_emails.as_deref().unwrap_or(text);
            let after_ips = if remove_ips { self.inner.replace_ips(current, &mut ips) } else { None };
            (after_ips.or(after_emails), emails.next, ips.next)
        })
    }
}
