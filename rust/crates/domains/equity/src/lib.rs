//! Equity services: the statement of changes in equity, which records each movement against its Book.
//!
//! Services here:
//! * `equity-changes-service`
//! * `authorized-share-capital-service` (records on the store: share classes, their issuances,
//!   buybacks and the used-capacity summary)
//!
//! One crate per business domain, not one per service: these services were separate Python
//! containers that each carried a copy of the same models, the same database access and the same
//! arithmetic. Grouped, a domain shares one build, one test run and one place to look.

#![forbid(unsafe_code)]

pub mod equity_changes;
pub mod share_classes;
pub use share_classes::{
    buyback as buyback_shares, create_class as create_share_class, get_class as get_share_class,
    issue as issue_shares, list_classes as list_share_classes,
    list_issuances as list_share_issuances, summary as share_capital_summary,
};
