#![forbid(unsafe_code)]
//! Cache service.
//!
//! * *cache-service* ([`cache`]): keyed entries with a life, a size limit, and the counts a cache keeps of itself.
//!
//! The shipped service kept its entries in one process's memory. Here they live on the store, so every replica sees the
//! same cache; the counting and the eviction order are its own either way.

pub mod cache;
