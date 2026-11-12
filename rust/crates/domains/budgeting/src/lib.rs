//! Budgeting services: appropriation control, which holds the appropriations and the commitments and expenditure charged against them.
//!
//! Services here:
//! * `appropriation-control-service`
//! * `profit-loss-account-service`
//! * `scenario-budget-service`
//!
//! One crate per business domain, not one per service: these services were separate Python
//! containers that each carried a copy of the same models, the same database access and the same
//! arithmetic. Grouped, a domain shares one build, one test run and one place to look.

#![forbid(unsafe_code)]

pub mod appropriation_control;
pub mod profit_loss;
pub mod scenario_budget;
