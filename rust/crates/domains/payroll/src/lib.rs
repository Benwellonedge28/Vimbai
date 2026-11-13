#![forbid(unsafe_code)]
//! Payroll services.
//!
//! * *benefits-admin-service* ([`benefits`]): the benefit plans a Book offers, who is enrolled in them, and the leave
//!   each person has accrued.
//!
//! The service keeps its records on the store, scoped to the caller and the Book, and is safe Rust by construction.

pub mod benefits;
