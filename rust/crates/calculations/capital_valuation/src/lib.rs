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
    analyze_synergies, calculate_disposal, deal_structuring_from_json, post_merger_from_json,
    structure_deal, synergy_analysis_from_json, track_integration, CostPerformance,
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
    book_value, book_value_from_json, calculate_capm, calculate_wacc, capm_from_json,
    comparable_valuation, dcf_valuation, dividend_discount, precedent_transaction,
    precedent_transaction_from_json, value_business, value_business_from_json, wacc_from_json,
    BookValueInput, BookValueOutput, CapmInput, CapmOutput, CompanyFinancials, ComparableCompany,
    ComparableMultiples, ComparableResult, DcfInputs, DcfResult, DdmResult, PrecedentInput,
    PrecedentOutput, ValuationOutput, ValuationRequest, WaccInput, WaccOutput,
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

/// `asset-allocation-service /allocate`.
///
/// The portfolio arrives as an object keyed by asset class, while `AssetAllocationInput` holds pairs.
/// Only the four target classes are read, so a holding outside them is ignored rather than weighted.
pub fn asset_allocation_service_from_json(
    payload: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let field_str = |key: &str| {
        payload
            .get(key)
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let portfolio = payload
        .get("current_portfolio")
        .and_then(serde_json::Value::as_object)
        .map(|map| {
            map.iter()
                .map(|(name, value)| (name.clone(), value.as_f64().unwrap_or(0.0)))
                .collect::<Vec<(String, f64)>>()
        })
        .unwrap_or_default();
    let input = allocation::AssetAllocationInput {
        company_id: field_str("company_id"),
        investor_profile: field_str("investor_profile"),
        time_horizon: payload
            .get("time_horizon")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0) as u32,
        risk_tolerance: field_str("risk_tolerance"),
        current_portfolio: portfolio,
        total_portfolio_value: payload
            .get("total_portfolio_value")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.0),
    };
    let mut out = serde_json::to_value(allocation::allocate_assets(&input))
        .map_err(|error| format!("could not serialise: {error}"))?;
    // The allocator holds these two as ordered pairs; the service answered with objects keyed by
    // asset class, and clients read them that way.
    if let Some(object) = out.as_object_mut() {
        for key in ["strategic_allocation", "tactical_allocation"] {
            if let Some(serde_json::Value::Array(pairs)) = object.get(key).cloned() {
                let mut mapped = serde_json::Map::new();
                for pair in pairs {
                    if let serde_json::Value::Array(kv) = pair {
                        if let (Some(name), Some(value)) = (kv.first(), kv.get(1)) {
                            if let Some(name) = name.as_str() {
                                mapped.insert(name.to_string(), value.clone());
                            }
                        }
                    }
                }
                object.insert(key.to_string(), serde_json::Value::Object(mapped));
            }
        }
    }
    Ok(out)
}

/// A uuid4-shaped identifier, matching the `id` the service gives each appraisal.
fn service_id() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let hex = format!("{nanos:032x}");
    format!(
        "{}-{}-4{}-a{}-{}",
        &hex[16..24],
        &hex[24..28],
        &hex[8..11],
        &hex[12..15],
        &hex[0..8]
    )
}

/// `project-appraisal-service /appraise`.
pub fn project_appraisal_service_from_json(
    payload: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let request = appraisal::AppraisalRequest {
        company_id: payload
            .get("company_id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string(),
        project_name: payload
            .get("project_name")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string(),
        initial_investment: payload
            .get("initial_investment")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.0),
        cash_flows: payload
            .get("cash_flows")
            .and_then(serde_json::Value::as_array)
            .map(|rows| rows.iter().map(|v| v.as_f64().unwrap_or(0.0)).collect())
            .unwrap_or_default(),
        discount_rate: payload
            .get("discount_rate")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.10),
        salvage_value: payload
            .get("salvage_value")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.0),
    };
    let mut out = serde_json::to_value(appraisal::project_appraisal(&request))
        .map_err(|error| format!("could not serialise: {error}"))?;
    if let Some(object) = out.as_object_mut() {
        object.insert("id".into(), serde_json::Value::String(service_id()));
    }
    Ok(out)
}

/// `portfolio-optimization-service /optimize`.
///
/// The service ignores `target_return` and `constraints`; so does this, deliberately. Assets arrive as
/// free-form objects, with missing fields falling back the way the service's `.get(...)` calls do.
pub fn portfolio_optimization_service_from_json(
    payload: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let assets: Vec<allocation::PortfolioAsset> = payload
        .get("assets")
        .and_then(serde_json::Value::as_array)
        .map(|rows| {
            rows.iter()
                .map(|asset| allocation::PortfolioAsset {
                    id: asset
                        .get("id")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    name: asset
                        .get("name")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    market_value: asset
                        .get("market_value")
                        .and_then(serde_json::Value::as_f64)
                        .unwrap_or(0.0),
                    expected_return: asset
                        .get("expected_return")
                        .and_then(serde_json::Value::as_f64)
                        .unwrap_or(0.1),
                })
                .collect()
        })
        .unwrap_or_default();
    let company_id = payload
        .get("company_id")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let risk_free_rate = payload
        .get("risk_free_rate")
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(0.0);
    serde_json::to_value(allocation::optimize_portfolio(
        company_id,
        &assets,
        risk_free_rate,
    ))
    .map_err(|error| format!("could not serialise: {error}"))
}

/// `capital-allocation-service /allocate`.
///
/// Projects default their discount rate to 10% and strategic value to 5, as the service's models do.
/// Selection is greedy in score order, so a project can be rejected while a smaller one below it fits.
pub fn capital_allocation_service_from_json(
    payload: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let projects: Vec<allocation::RationProject> = payload
        .get("projects")
        .and_then(serde_json::Value::as_array)
        .map(|rows| {
            rows.iter()
                .map(|project| allocation::RationProject {
                    name: project
                        .get("name")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    initial_investment: project
                        .get("initial_investment")
                        .and_then(serde_json::Value::as_f64)
                        .unwrap_or(0.0),
                    annual_cash_flows: project
                        .get("annual_cash_flows")
                        .and_then(serde_json::Value::as_array)
                        .map(|flows| flows.iter().map(|v| v.as_f64().unwrap_or(0.0)).collect())
                        .unwrap_or_default(),
                    discount_rate: project
                        .get("discount_rate")
                        .and_then(serde_json::Value::as_f64)
                        .unwrap_or(0.10),
                    strategic_value: project
                        .get("strategic_value")
                        .and_then(serde_json::Value::as_i64)
                        .unwrap_or(5) as i32,
                })
                .collect()
        })
        .unwrap_or_default();
    let company_id = payload
        .get("company_id")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let capital_budget = payload
        .get("capital_budget")
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(0.0);
    let mut out = serde_json::to_value(allocation::allocate_capital(
        company_id,
        capital_budget,
        &projects,
    ))
    .map_err(|error| format!("could not serialise: {error}"))?;
    if let Some(object) = out.as_object_mut() {
        object.insert("id".into(), serde_json::Value::String(service_id()));
    }
    Ok(out)
}

/// `investment-appraisal-service /appraise`.
///
/// `evaluation_criteria` is accepted and ignored, as the service does. Missing project fields fall
/// back to the service's `.get` defaults: 7 for the strategic, environmental, social and governance
/// scores, 0.5 for risk, 0 for NPV.
pub fn investment_appraisal_service_from_json(
    payload: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let projects: Vec<appraisal::ScoredProjectInput> = payload
        .get("projects")
        .and_then(serde_json::Value::as_array)
        .map(|rows| {
            rows.iter()
                .map(|project| {
                    let field = |key: &str, fallback: f64| {
                        project
                            .get(key)
                            .and_then(serde_json::Value::as_f64)
                            .unwrap_or(fallback)
                    };
                    appraisal::ScoredProjectInput {
                        id: project
                            .get("id")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        npv: field("npv", 0.0),
                        strategic_value: field("strategic_value", 7.0),
                        risk_level: field("risk_level", 0.5),
                        environmental_impact: field("environmental_impact", 7.0),
                        social_impact: field("social_impact", 7.0),
                        governance_score: field("governance_score", 7.0),
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    let weights_json = payload
        .get("weights")
        .and_then(serde_json::Value::as_object);
    let weight = |key: &str, fallback: f64| {
        weights_json
            .and_then(|map| map.get(key))
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(fallback)
    };
    let weights = appraisal::AppraisalWeights {
        financial: weight("financial", 0.4),
        strategic: weight("strategic", 0.2),
        risk: weight("risk", 0.2),
        esg: weight("esg", 0.2),
    };
    let company_id = payload
        .get("company_id")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    serde_json::to_value(appraisal::appraise_investments(
        company_id, &projects, &weights,
    ))
    .map_err(|error| format!("could not serialise: {error}"))
}
