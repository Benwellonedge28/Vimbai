#![forbid(unsafe_code)]
//! Audit services.
//!
//! * *audit-engagement-service* ([`engagement`]): the engagements an auditor runs, the findings raised during them,
//!   and the report those findings add up to.
//! * *audit-sampling-service* ([`sampling`]): the sample an auditor draws from a population, by attribute or by
//!   value, stratified or not.
//! * *audit-compliance-service* ([`compliance`]): the immutable trail, resource versions, lineage, chain integrity,
//!   and the reports and assessments drawn from them.
//!
//! An engagement owns records; a sample is answered and forgotten, which is what the shipped service did with it.

pub mod compliance;
pub mod engagement;
pub mod sampling;
