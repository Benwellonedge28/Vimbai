//! The administrative seat: what is switched on, for whom, since when, and who said so.
//!
//! * *admin-service* ([`admin`]): the feature catalogue and each caller's own overrides of it, organisation-level
//!   configuration, rollout schedules, the requests people raise for new features, system configuration and the audit
//!   trail that records every change.

#![forbid(unsafe_code)]

pub mod admin;
