#![forbid(unsafe_code)]
//! Security services.
//!
//! * *zero-trust-data-service* ([`zero_trust`]): the access policies a Book sets and every decision taken against
//!   them, kept as a record so an access question can be answered after the fact.
//!
//! The service keeps its records on the store, scoped to the caller and the Book, and is safe Rust by construction.

pub mod zero_trust;
