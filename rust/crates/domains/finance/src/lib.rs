//! Finance services: insurance claims, where a claim is settled against its policy and the settlement is recorded.
//!
//! Services here:
//! * `insurance-claims-service`
//!
//! One crate per business domain, not one per service: these services were separate Python
//! containers that each carried a copy of the same models, the same database access and the same
//! arithmetic. Grouped, a domain shares one build, one test run and one place to look.

#![forbid(unsafe_code)]

pub mod insurance_claims;
