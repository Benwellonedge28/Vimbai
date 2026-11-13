#![forbid(unsafe_code)]
//! Identity services.
//!
//! * *financial-identity-service* ([`kyc`]): the profile a Book keeps for a person - the legal name, the identifiers,
//!   where they live, what they earn - and the verification done against it.
//!
//! The service keeps its records on the store, scoped to the caller and the Book, and is safe Rust by construction.

pub mod kyc;
