#![forbid(unsafe_code)]
//! Notifications service.
//!
//! * *notifications-service* ([`notifications`]): each person's own inbox, the preferences they set for it, and the
//!   templates that fill a notice in.
//!
//! The service keeps its records on the store, each recipient owning their own copy, and is safe Rust by construction.

pub mod notifications;
