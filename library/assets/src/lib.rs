#![forbid(unsafe_code)]
//! Asset math: depreciation, amortization, inventory valuation, net
//! realizable value, revaluation and goodwill impairment.
//!
//! Ported from: depreciation-service, amortization-service,
//! inventory-valuation-service, net-realizable-value-service,
//! revaluation-model-service and goodwill-service.
//!
//! Remaining services queued for this crate: fixed-assets,
//! fixed-assets-register, fixed-assets-schedule, investment-property,
//! biological-assets, intangible-assets, disposal-account,
//! revaluation-reserve.

pub mod amortization;
pub mod depreciation;
pub mod goodwill;
pub mod inventory;
pub mod nrv;
pub mod revaluation;

pub use amortization::{amortization, AmortizationAsset, AmortizationOutput, AmortizationRow};
pub use depreciation::{
    depreciation_batch, depreciation_method, DepreciationAsset, DepreciationMethodOutput,
    DepreciationOutput, MethodScheduleRow,
};
pub use goodwill::{impairment_test, Cgu, ImpairmentResult, TestMethod};
pub use inventory::{inventory_valuation, InventoryOutput, Purchase, Sale, ValuationMethod};
pub use nrv::{
    compare_cost_nrv, nrv, residual_value, scrap_value, CostNrvComparison, NrvOutput,
    ResidualValueOutput, ScrapValueOutput,
};
pub use revaluation::{revalue, AssetBook, RevaluationAsset, RevaluationOutput, RevaluationRow};
