#![forbid(unsafe_code)]
//! Alerts service.
//!
//! * *alerts-service* ([`alerts`]): the rules a Book watches - a threshold, a pattern, a deadline - and the alerts
//!   those rules raise, with what became of each one.
//!
//! The service keeps its records on the store, scoped to the caller and the Book, and is safe Rust by construction.

pub mod alerts;
