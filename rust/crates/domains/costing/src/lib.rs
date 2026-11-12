//! Costing services: absorption costing, process and product costing, overhead absorption rates, basis apportionment, job costing, CVP analysis and selling price reduction. Every service in this crate owns its own records and shares the store's Book scoping.
//!
//! Services here:
//! * `absorption-costing-service`
//! * `basis-apportionment-service`
//! * `cvp-analysis-service`
//! * `job-costing-service`
//! * `overhead-absorption-rate-service`
//! * `process-costing-service`
//! * `product-costing-service`
//! * `selling-price-reduction-service`
//!
//! One crate per business domain, not one per service: these services were separate Python
//! containers that each carried a copy of the same models, the same database access and the same
//! arithmetic. Grouped, a domain shares one build, one test run and one place to look.

#![forbid(unsafe_code)]

pub mod absorption;
pub mod basis_apportionment;
pub mod cost_calculations;
pub mod cvp;
pub mod job_costing;
pub mod overhead_absorption;
pub mod price_reduction;
