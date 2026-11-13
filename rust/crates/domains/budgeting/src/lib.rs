//! Budgeting services: appropriation control, which holds the appropriations and the commitments and expenditure charged against them.
//!
//! Services here:
//! * `appropriation-control-service`
//! * `profit-loss-account-service`
//! * `scenario-budget-service`
//! * `budget-service`
//! * `zero-based-budgeting-service`
//! * `rolling-forecast-service`
//! * `scenario-analysis-service`
//! * `sensitivity-analysis-service`
//! * `scenario-modeling-service`
//!
//! One crate per business domain, not one per service: these services were separate Python
//! containers that each carried a copy of the same models, the same database access and the same
//! arithmetic. Grouped, a domain shares one build, one test run and one place to look.

#![forbid(unsafe_code)]

pub mod appropriation_control;
pub mod figures;
pub mod master_budget;
pub mod profit_loss;
pub mod rolling_forecast;
pub mod scenario_analysis;
pub mod scenario_budget;
pub mod scenario_modeling;
pub mod sensitivity_analysis;
pub mod zero_based_budget;
