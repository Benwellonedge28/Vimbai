#![forbid(unsafe_code)]
//! Compliance services.
//!
//! Three services live here, because they are three answers to one question - what the Book is obliged to control,
//! what it is exposed to, and which of those obligations it is measured against:
//!
//! * *sox-compliance-service* ([`sox`]): the controls a Book registers, the tests run against them, and the
//!   deficiencies those tests raise.
//! * *risk-assessment-service* ([`risks`]): the risk register, scored by likelihood and impact.
//! * *regulatory-compliance-service* ([`regulation`]): the regulations a company is held to, the state of each, and
//!   the dashboard that adds them up.
//!
//! All three keep their records on the store, scoped to the caller and the Book, and all three are safe Rust by
//! construction.

pub mod regulation;
pub mod risks;
pub mod sox;
