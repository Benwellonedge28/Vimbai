//! The financial-integrity suite lives in `tests/`; this library carries the crate's own promise.
//!
//! The suite is a crate of its own, outside the kernel and the domains it audits, so a test can fail because the ledger
//! stopped guaranteeing something rather than because a colleague changed an expectation in the same file as the rule.
#![forbid(unsafe_code)]
