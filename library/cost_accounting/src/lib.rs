#![forbid(unsafe_code)]
//! Cost and management accounting math.
//!
//! First batch ported from: cvp-analysis-service, marginal-costing-service,
//! overhead-absorption-rate-service, over-under-absorption-service,
//! equivalent-units-service and absorption-costing-service (calculation
//! core only; persistence and journal-entry side effects stay in the
//! Python services until the seam is wired).
//!
//! Remaining services for this crate: fixed-cost, variable-cost
//! (their aggregation logic is persistence-bound; the pure parts
//! reduce to sums covered by other modules).

pub mod absorption;
pub mod activity_based;
pub mod actual_cost;
pub mod budgeted_cost;
pub mod cost_accounting_core;
pub mod cost_centre;
pub mod job_costing;
pub mod lifecycle;
pub mod limiting_factor;
pub mod make_or_buy;
pub mod order_acceptance;
pub mod overhead_apportionment;
pub mod prime_cost;
pub mod process_product_costing;
pub mod standard_cost;
pub mod target_costing;
pub mod throughput;
pub mod total_production_cost;

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
