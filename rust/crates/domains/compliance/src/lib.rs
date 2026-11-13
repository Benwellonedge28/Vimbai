#![forbid(unsafe_code)]
//! Compliance services.
//!
//! Four services live here, because they are four answers to one question - what the Book is obliged to control, what it
//! is exposed to, which of those obligations it is measured against, and what it owes the people whose data it holds:
//!
//! * *sox-compliance-service* ([`sox`]): the controls a Book registers, the tests run against them, and the
//!   deficiencies those tests raise.
//! * *risk-assessment-service* ([`risks`]): the risk register, scored by likelihood and impact.
//! * *regulatory-compliance-service* ([`regulation`]): the regulations a company is held to, the state of each, and
//!   the dashboard that adds them up.
//! * *privacy-admin-dashboard-service* ([`privacy`]): the data subject requests a Book must answer, the consent it must
//!   respect, and the register that adds up what it owes and what it refused.
//!
//! All four keep their records on the store, scoped to the caller and the Book, and all four are safe Rust by
//! construction.

pub mod privacy;
pub mod regulation;
pub mod risks;
pub mod sox;
