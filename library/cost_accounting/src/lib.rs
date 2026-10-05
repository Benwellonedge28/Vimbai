#![forbid(unsafe_code)]
//! Cost and management accounting math.
//!
//! First batch ported from: cvp-analysis-service, marginal-costing-service,
//! overhead-absorption-rate-service, over-under-absorption-service,
//! equivalent-units-service and absorption-costing-service (calculation
//! core only; persistence and journal-entry side effects stay in the
//! Python services until the seam is wired).
//!
//! Remaining services queued for this crate: activity-based-costing,
//! cost-accounting, cost-centre, fixed-cost, variable-cost,
//! lifecycle-costing, limiting-factor, make-or-buy, order-acceptance,
//! process-costing, product-costing, total-production-cost,
//! overhead-apportionment.

pub mod absorption;
pub mod actual_cost;
pub mod budgeted_cost;
pub mod job_costing;
pub mod prime_cost;
pub mod standard_cost;
pub mod target_costing;
pub mod throughput;

pub mod cvp;
pub mod equivalent_units;
pub mod marginal;
pub mod overhead;

pub use absorption::{
    cost_plus_pricing, overhead_absorption, product_cost, CostPlusOutput, OverheadAbsorptionOutput,
    ProductCostOutput,
};
pub use cvp::{
    contribution, multi_product_cvp, perform_cvp, quick_cvp, target_profit, ContributionOutput,
    CvpAnalysis, MultiProductItem, MultiProductOutput, ProductBreakdown, QuickCvpOutput,
    TargetProfitOutput,
};
pub use equivalent_units::{
    equivalent_units, EquivalentUnitsMethod, EquivalentUnitsOutput, EquivalentUnitsRequest,
};
pub use marginal::{
    contribution_analysis, forecast_profit, marginal_cost_item, marginal_income_statement,
    ContributionAnalysisOutput, MarginalCostOutput, MarginalIncomeStatementOutput,
};
pub use overhead::{
    labour_cost_rate, labour_hours_rate, machine_hours_rate, material_cost_rate, oar,
    over_under_absorption, simple_over_under, spending_variance, volume_variance, Cause, OarOutput,
    OverUnderOutput, RateOutput, SimpleOverUnderOutput, SpendingVarianceOutput,
    VolumeVarianceOutput,
};
