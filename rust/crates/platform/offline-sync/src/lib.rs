#![forbid(unsafe_code)]
//! Book sync.
//!
//! * *book-sync-service* ([`sync`]): the shared Books a person belongs to, who else is in them, their devices, and the
//!   encrypted entries those devices push and pull.
//!
//! A shared Book's rows belong to the Book rather than to one person, so this is the one place that uses the store's
//! Book-keyed records - and every entry point checks membership before it touches them.

pub mod conflict;
pub mod sync;
