//! Treasury services: treasury analytics, cash optimisation, and cash management. Snapshots and the
//! recommendations drawn from them, the accounts money sits in, the transfers between them and the liquidity
//! they add up to, all Book-scoped.
//!
//! Services here:
//! * `cash-management-service`
//! * `cash-optimization-service`
//! * `treasury-analytics-service`
//!
//! One crate per business domain, not one per service: these services were separate Python
//! containers that each carried a copy of the same models, the same database access and the same
//! arithmetic. Grouped, a domain shares one build, one test run and one place to look.

#![forbid(unsafe_code)]

pub mod cash_management;
pub mod cash_optimization;
pub mod treasury_analytics;
