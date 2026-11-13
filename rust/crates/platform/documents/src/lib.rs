#![forbid(unsafe_code)]
//! Document service.
//!
//! * *document-service* ([`documents`]): the files a Book holds - what each one is, its checksum so the same content
//!   cannot be filed twice, the text taken off it, and what it is linked to.
//!
//! The service keeps its records on the store, scoped to the caller and the Book, and is safe Rust by construction.

pub mod documents;
