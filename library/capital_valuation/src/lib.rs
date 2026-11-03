#![forbid(unsafe_code)]
//! Capital budgeting & valuation math.
//!
//! Ported from net-present-value-service, internal-rate-return-service,
//! payback-period-service, discount-factor-service, time-value-of-money,
//! present-value, mva, sustainable-growth, asset-allocation,
//! portfolio-optimization, capital-allocation, investment-appraisal and
//! project-appraisal services, with exact formula and iteration parity.
//! The remaining capital/valuation services
//! (accounting-rate-return, cost-of-capital, business/merger-valuation,
//! EVA, CROIC, MVA, du-pont, asset-allocation, asset-turnover,
//! risk-return, sustainable-growth, initial-investment,
//! investment-appraisal, capital-budgeting, deal-structuring,
//! divestiture, post-merger, synergy-analysis) are queued to port here;
//! see `queued.rs` for their formula inventory.

pub mod allocation;
pub mod appraisal;
pub mod capital_budgeting;
pub mod capital_returns;
pub mod dcf;
pub mod growth;
pub mod irr;
pub mod payback;
pub mod queued;
pub mod time_value;

pub use allocation::{
    allocate_assets, allocate_capital, optimize_portfolio, tactical_allocation, AllocationResult,
    AssetAllocationInput, AssetAllocationOutput, CapitalAllocationOutput, FrontierPoint,
    OptimizedAllocation, PortfolioAsset, PortfolioOptimizationOutput, RationProject, ScoredProject,
};
pub use appraisal::{
    appraise_investments, project_appraisal, AppraisalRequest, AppraisalResult, AppraisalWeights,
    CashFlowAnalysis, InvestmentAppraisalOutput, ProjectAppraisalOutput, ScoredProjectInput,
};
pub use capital_budgeting::{
    analyse_capital_budget, AppraisedProject, CapitalBudgetRequest, CapitalBudgetResult,
    ProjectInput,
};
pub use dcf::{
    annuity_factor, discount_factor, discount_factor_percent, discount_factor_table,
    discount_factor_table_range, npv, npv_percent, npv_project_comparison, profitability_index,
    NpvDecision, NpvDetail, NpvOutput, ProjectNpv,
};
pub use growth::{
    market_value_added, sustainable_growth, GrowthComparison, GrowthInput, GrowthMetrics,
    GrowthOutput, MvaInput, MvaMetrics, MvaOutput, ShareholderValueAnalysis,
};
pub use irr::{irr, irr_bisection, npv_at_rate, IrrMethod, IrrOutput};
pub use payback::{payback_period, PaybackOutput};
pub use time_value::{
    compound_interest, effective_annual_rate, future_value, fv_annuity, loan_repayment,
    present_value, present_value_growing_perpetuity, present_value_of_cash_flows,
    present_value_percent, present_value_perpetuity, present_value_with_initial_investment,
    pv_annuity, simple_interest,
};
