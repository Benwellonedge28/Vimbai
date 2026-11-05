//! Vimbai group accounting.
//!
//! Partnerships, a partner leaving, business combinations, goodwill impairment,
//! defined benefit pensions, and consolidation. Stateless: no database, no clock.
//!
//! Every module documents the quirks it preserves. `consolidation` is the one
//! place that departs from its service, because the service invented the parent's
//! figures and a cash flow statement; see the module docs.

#![forbid(unsafe_code)]

pub mod buyout;
pub mod combination;
pub mod consolidation;
pub mod consolidation_views;
pub mod divestiture;
pub mod impairment;
pub mod partnership;
pub mod pension;

pub use buyout::{
    goodwill, settle, BuyoutRequest, BuyoutResult, GoodwillRequest, GoodwillResult,
    RemainingPartner,
};
pub use combination::{acquire, AcquisitionRequest, AcquisitionResult, IdentifiableAsset};
pub use consolidation::{
    consolidate, ConsolidationRequest, ConsolidationResult, IntercompanyTransaction,
    ParentFinancials, SubsidiaryFinancials,
};
pub use impairment::{
    test as test_impairment, Cgu, ImpairmentRequest, ImpairmentResult, TestMethod,
};
pub use partnership::{allocate, Partner, PartnerAccount, PartnershipRequest, PartnershipResult};
pub use pension::{calculate as calculate_pension, PensionRequest, PensionResult};
pub use vimbai_common::numbers::{money, round_dp};

#[cfg(test)]
mod tests;
