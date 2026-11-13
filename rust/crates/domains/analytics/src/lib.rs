#![forbid(unsafe_code)]
//! Analytics services.
//!
//! What a Book measures itself against, itemised:
//!
//! * *performance-benchmarking-service* ([`benchmarking`]): the metrics an industry is measured on, where one Book falls
//!   against them, and the peers it is compared with.
//!
//! Records live on the store, scoped to the caller and the Book, and the crate is safe Rust by construction.

pub mod benchmarking;
