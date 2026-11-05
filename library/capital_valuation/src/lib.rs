#![forbid(unsafe_code)]
//! Capital budgeting & valuation math.
//!
//! Ported from net-present-value-service, internal-rate-return-service,
//! payback-period-service, discount-factor-service, time-value-of-money,
//! present-value, mva, sustainable-growth, asset-allocation,
//! portfolio-optimization, capital-allocation, investment-appraisal and
//! project-appraisal services, with exact formula and iteration parity.
//! Second batch: risk-return-analysis, business-valuation and
//! merger-valuation in `risk_return.rs`, `valuation.rs` and `merger.rs`.
//! Third batch: deal-structuring, divestiture, post-merger and
//! synergy-analysis in `corporate_events.rs`. With those the capital and
//! valuation family is fully ported; see `queued.rs` for what remains.

pub mod acquisition_financing;
pub mod allocation;
pub mod appraisal;
pub mod capital_budgeting;
pub mod capital_returns;
pub mod corporate_events;
pub mod dcf;
pub mod growth;
pub mod irr;
pub mod merger;
pub mod payback;
pub mod queued;
pub mod risk_return;
pub mod time_value;
pub mod valuation;

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
pub use corporate_events::{
    analyze_synergies, calculate_disposal, structure_deal, track_integration, CostPerformance,
    DealStructuringOutput, DealStructuringRequest, DivestitureOutput, DivestitureRequest,
    IntegrationMilestone, IntegrationProgress, PostMergerOutput, PostMergerRequest, RiskAllocation,
    RiskSplit, StructureOption, SynergyAnalysisOutput, SynergyAnalysisRequest, SynergyItem,
    SynergyPerformance, SynergySummary, TaxConsiderations, TaxView,
};
pub use dcf::{
    npv, npv_percent, npv_project_comparison, profitability_index, NpvDecision, NpvDetail,
    NpvOutput, ProjectNpv,
};
pub use growth::{
    market_value_added, sustainable_growth, GrowthComparison, GrowthInput, GrowthMetrics,
    GrowthOutput, MvaInput, MvaMetrics, MvaOutput, ShareholderValueAnalysis,
};
pub use irr::{irr, irr_bisection, npv_at_rate, IrrMethod, IrrOutput};
pub use merger::{
    value_merger, IrrProjections, MergerOutput, MergerRequest, ValuationMethod, ValuationMethods,
};
pub use payback::{payback_period, PaybackOutput};
pub use risk_return::{
    analyze_risk_return, AssetInput, RiskReturnFrontierPoint, RiskReturnMetrics, RiskReturnOutput,
    RiskReturnRequest,
};
pub use time_value::{
    compound_interest, effective_annual_rate, future_value, fv_annuity, loan_repayment,
    present_value, present_value_growing_perpetuity, present_value_of_cash_flows,
    present_value_percent, present_value_perpetuity, present_value_with_initial_investment,
    pv_annuity, simple_interest,
};
pub use valuation::{
    book_value, calculate_capm, calculate_wacc, comparable_valuation, dcf_valuation,
    dividend_discount, precedent_transaction, value_business, BookValueInput, BookValueOutput,
    CapmInput, CapmOutput, CompanyFinancials, ComparableCompany, ComparableMultiples,
    ComparableResult, DcfInputs, DcfResult, DdmResult, PrecedentInput, PrecedentOutput,
    ValuationOutput, ValuationRequest, WaccInput, WaccOutput,
};

/// `acquisition-financing-service /structure`.
pub fn acquisition_financing_from_json(
    payload: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    acquisition_financing::structure(payload)
}

/// `mva-service /calculate`.
pub fn mva_service_from_json(payload: &serde_json::Value) -> Result<serde_json::Value, String> {
    let input: growth::MvaInput = serde_json::from_value(payload.clone())
        .map_err(|error| format!("invalid request: {error}"))?;
    serde_json::to_value(growth::market_value_added(&input))
        .map_err(|error| format!("could not serialise: {error}"))
}

/// `sustainable-growth-service /calculate`.
pub fn sustainable_growth_service_from_json(
    payload: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let input: growth::GrowthInput = serde_json::from_value(payload.clone())
        .map_err(|error| format!("invalid request: {error}"))?;
    serde_json::to_value(growth::sustainable_growth(&input))
        .map_err(|error| format!("could not serialise: {error}"))
}
