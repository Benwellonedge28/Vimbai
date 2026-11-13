#![forbid(unsafe_code)]
//! Compliance services.
//!
//! Two services live here, because they are two answers to one question - what the Book is obliged to control and
//! what it is exposed to:
//!
//! * *sox-compliance-service* ([`sox`]): the controls a Book registers, the tests run against them, and the
//!   deficiencies those tests raise.
//! * *risk-assessment-service* ([`risks`]): the risk register, scored by likelihood and impact.
//!
//! Both keep their records on the store, scoped to the caller and the Book, and both are safe Rust by
//! construction.

pub mod risks;
pub mod sox;
