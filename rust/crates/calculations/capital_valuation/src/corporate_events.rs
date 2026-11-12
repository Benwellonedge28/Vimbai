#![forbid(unsafe_code)]
//! Corporate events: deal structuring, divestiture, post-merger tracking and
//! synergy analysis. Ported from deal-structuring-service,
//! divestiture-service, post-merger-service and synergy-analysis-service.
//!
//! These four are decision support, not accounting engines, and three of them
//! are largely fixed tables. Kept as shipped:
//!
//! * Deal structuring offers three structures whose payments are fixed
//!   fractions of the deal value (cash pays all upfront; share exchange pays
//!   80% and earns out 20%; hybrid splits 60/20/20) with fixed tax, speed and
//!   risk scores. The "recommended" structure is chosen from the seller's
//!   preference alone and the tax/risk tables are constants.
//! * Post-merger status bands are 90% and 70% of target, and the traffic light
//!   is Red as soon as two risk areas fire.
//! * Synergy analysis discounts each item by `(1 + wacc)^(months / 12)`,
//!   inflates integration costs by a flat 10%, and reports a three-year CAGR as
//!   "irr" rather than solving for a rate.
//!
//! Divestiture is the one with real arithmetic: the gain on disposal is net
//! proceeds less carrying value *plus* the goodwill that leaves the group, and
//! a retained interest adds its fair value to the recognised gain. Its journal
//! lines are reproduced as text, including Python's `600.0` style for whole
//! numbers.
//!
//! The two divisions the Python left unguarded (milestones-per-day and the
//! three-year CAGR on zero integration costs) yield 0.0 rather than crashing.

use serde::{Deserialize, Serialize};
use vimbai_common::{py_round, Money};

fn r2(value: f64) -> f64 {
    py_round(value, 2)
}

fn rate(value: f64) -> Money {
    Money::from_f64_shortest(value).unwrap_or_else(|_| Money::zero())
}

/// Python's `str(float)` for the values used in journal text: whole numbers keep
/// one decimal ("600.0"), others print their shortest form.
fn py_float_str(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e15 {
        format!("{value:.1}")
    } else {
        let mut text = format!("{value}");
        if text.contains('e') {
            text = format!("{value:.2}");
        }
        text
    }
}

// ------------------------------------------------------- deal structuring

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DealStructuringRequest {
    pub deal_id: String,
    pub target_id: String,
    pub total_value: Money,
    #[serde(default)]
    pub seller_preference: String,
    #[serde(default)]
    pub tax_efficiency_priority: bool,
    #[serde(default)]
    pub earnout_required: bool,
    #[serde(default)]
    pub escrow_percentage: f64,
    #[serde(default)]
    pub locked_box_date: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StructureOption {
    pub structure_type: String,
    pub upfront_payment: Money,
    pub deferred_payment: Money,
    pub earnout: Money,
    pub escrow: Money,
    pub tax_efficiency: f64,
    pub speed_close: f64,
    pub risk_transfer: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaxView {
    pub capital_gains_exposure: String,
    pub step_up_benefit: bool,
    pub liability_assumption: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaxConsiderations {
    pub asset_deal: TaxView,
    pub share_deal: TaxView,
    pub recommended: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RiskSplit {
    pub buyer: f64,
    pub seller: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RiskAllocation {
    pub completion_risk: RiskSplit,
    pub earnout_risk: RiskSplit,
    pub warranty_risk: RiskSplit,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DealStructuringOutput {
    pub deal_id: String,
    pub deal_date: String,
    pub structure_options: Vec<StructureOption>,
    pub recommended_structure: StructureOption,
    pub tax_considerations: TaxConsiderations,
    pub risk_allocation: RiskAllocation,
    pub recommendations: Vec<String>,
}

pub fn structure_deal(request: &DealStructuringRequest, deal_date: &str) -> DealStructuringOutput {
    let total = request.total_value.clone();

    let cash_deal = StructureOption {
        structure_type: "All Cash".to_string(),
        upfront_payment: total.clone(),
        deferred_payment: Money::zero(),
        earnout: Money::zero(),
        escrow: total.times(&rate(0.1)).round(2),
        tax_efficiency: 0.7,
        speed_close: 0.9,
        risk_transfer: 0.8,
    };

    let share_deal = StructureOption {
        structure_type: "Share Exchange".to_string(),
        upfront_payment: total.times(&rate(0.8)).round(2),
        deferred_payment: Money::zero(),
        earnout: total.times(&rate(0.2)).round(2),
        escrow: total.times(&rate(0.05)).round(2),
        tax_efficiency: 0.9,
        speed_close: 0.5,
        risk_transfer: 0.6,
    };

    let hybrid = StructureOption {
        structure_type: "Hybrid (Cash + Stock)".to_string(),
        upfront_payment: total.times(&rate(0.6)).round(2),
        deferred_payment: total.times(&rate(0.2)).round(2),
        earnout: total.times(&rate(0.2)).round(2),
        escrow: total.times(&rate(0.08)).round(2),
        tax_efficiency: 0.85,
        speed_close: 0.7,
        risk_transfer: 0.7,
    };

    let recommended = match request.seller_preference.as_str() {
        "cash" => cash_deal.clone(),
        "stock" => share_deal.clone(),
        _ => hybrid.clone(),
    };

    let tax_recommended = if request.tax_efficiency_priority {
        "Hybrid"
    } else if request.seller_preference == "stock" {
        "Share"
    } else {
        "Cash"
    };

    let mut recommendations = Vec::new();
    if request.tax_efficiency_priority {
        recommendations.push("Consider share exchange to optimize seller tax position".to_string());
    }
    if request.earnout_required {
        recommendations.push("Structure earnout with clear KPIs and caps".to_string());
    }
    if request.escrow_percentage < 0.1 {
        recommendations.push("Consider higher escrow for warranty protection".to_string());
    }

    DealStructuringOutput {
        deal_id: request.deal_id.clone(),
        deal_date: deal_date.to_string(),
        structure_options: vec![cash_deal, share_deal, hybrid],
        recommended_structure: recommended,
        tax_considerations: TaxConsiderations {
            asset_deal: TaxView {
                capital_gains_exposure: "High".to_string(),
                step_up_benefit: true,
                liability_assumption: true,
            },
            share_deal: TaxView {
                capital_gains_exposure: "Medium".to_string(),
                step_up_benefit: false,
                liability_assumption: false,
            },
            recommended: tax_recommended.to_string(),
        },
        risk_allocation: RiskAllocation {
            completion_risk: RiskSplit {
                buyer: 0.7,
                seller: 0.3,
            },
            earnout_risk: RiskSplit {
                buyer: 0.5,
                seller: 0.5,
            },
            warranty_risk: RiskSplit {
                buyer: 0.3,
                seller: 0.7,
            },
        },
        recommendations,
    }
}

// ------------------------------------------------------------ divestiture

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DivestitureRequest {
    pub company_id: String,
    pub subsidiary_name: String,
    pub carrying_value: f64,
    pub disposal_proceeds: f64,
    #[serde(default)]
    pub disposal_costs: f64,
    #[serde(default)]
    pub retained_interest_fair_value: f64,
    #[serde(default)]
    pub retained_interest_pct: f64,
    #[serde(default)]
    pub associated_goodwill: f64,
    #[serde(default = "sale_method")]
    pub disposal_method: String,
}

fn sale_method() -> String {
    "sale".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DivestitureOutput {
    pub company_id: String,
    pub subsidiary_name: String,
    pub disposal_method: String,
    pub carrying_value: f64,
    pub net_proceeds: f64,
    pub gain_or_loss: f64,
    pub retained_interest_fv: f64,
    pub total_gain_recognized: f64,
    pub goodwill_derecognized: f64,
    pub journal_entries: Vec<String>,
}

pub fn calculate_disposal(request: &DivestitureRequest) -> DivestitureOutput {
    let net_proceeds = request.disposal_proceeds - request.disposal_costs;
    let gain_loss = net_proceeds - (request.carrying_value - request.associated_goodwill);

    let (total_gain, mut entries) =
        if request.retained_interest_pct > 0.0 && request.retained_interest_fair_value > 0.0 {
            let total = gain_loss + request.retained_interest_fair_value;
            let mut lines = vec![
                format!("Dr Cash/Bank ({})", py_float_str(r2(net_proceeds))),
                format!(
                    "Dr Investment in Subsidiary (retained) ({})",
                    py_float_str(r2(request.retained_interest_fair_value))
                ),
                format!(
                    "Cr Investment in Subsidiary ({})",
                    py_float_str(r2(request.carrying_value))
                ),
            ];
            lines.push(gain_line(total));
            if request.associated_goodwill > 0.0 {
                lines.push("Cr Goodwill derecognized".to_string());
            }
            (total, lines)
        } else {
            let mut lines = vec![
                format!("Dr Cash/Bank ({})", py_float_str(r2(net_proceeds))),
                format!(
                    "Cr Investment in Subsidiary ({})",
                    py_float_str(r2(request.carrying_value))
                ),
            ];
            lines.push(gain_line(gain_loss));
            if request.associated_goodwill > 0.0 {
                lines.push("Dr/Cr Goodwill adjustment".to_string());
            }
            (gain_loss, lines)
        };

    entries.retain(|line| !line.is_empty());

    DivestitureOutput {
        company_id: request.company_id.clone(),
        subsidiary_name: request.subsidiary_name.clone(),
        disposal_method: request.disposal_method.clone(),
        carrying_value: r2(request.carrying_value),
        net_proceeds: r2(net_proceeds),
        gain_or_loss: r2(gain_loss),
        retained_interest_fv: r2(request.retained_interest_fair_value),
        total_gain_recognized: r2(total_gain),
        goodwill_derecognized: r2(request.associated_goodwill),
        journal_entries: entries,
    }
}

fn gain_line(total: f64) -> String {
    if total > 0.0 {
        format!("Cr Gain on Disposal ({})", py_float_str(r2(total)))
    } else {
        format!("Dr Loss on Disposal ({})", py_float_str(r2(total.abs())))
    }
}

// ------------------------------------------------------------ post-merger

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IntegrationMilestone {
    pub milestone_id: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub planned_date: String,
    #[serde(default)]
    pub actual_date: String,
    pub status: String,
    #[serde(default)]
    pub completion_percentage: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PostMergerRequest {
    pub deal_id: String,
    #[serde(default)]
    pub acquirer_id: String,
    #[serde(default)]
    pub target_id: String,
    pub synergies_achieved: Money,
    pub synergies_target: Money,
    pub integration_costs_actual: Money,
    pub integration_costs_budget: Money,
    #[serde(default)]
    pub days_since_close: i64,
    #[serde(default)]
    pub milestones: Vec<IntegrationMilestone>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SynergyPerformance {
    pub target: Money,
    pub achieved: Money,
    pub rate: f64,
    pub variance: Money,
    pub status: String,
    pub daily_rate: Money,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CostPerformance {
    pub budget: Money,
    pub actual: Money,
    pub variance: Money,
    pub variance_percentage: f64,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IntegrationProgress {
    pub total_milestones: usize,
    pub completed: usize,
    pub in_progress: usize,
    pub delayed: usize,
    pub average_completion: f64,
    pub days_since_close: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PostMergerOutput {
    pub deal_id: String,
    pub status_date: String,
    pub synergy_performance: SynergyPerformance,
    pub cost_performance: CostPerformance,
    pub integration_progress: IntegrationProgress,
    pub risk_areas: Vec<String>,
    pub overall_status: String,
    pub recommendations: Vec<String>,
}

pub fn track_integration(request: &PostMergerRequest, status_date: &str) -> PostMergerOutput {
    let synergy_rate = if request.synergies_target.is_zero() {
        0.0
    } else {
        request
            .synergies_achieved
            .over(&request.synergies_target)
            .to_f64_lossy()
    };
    let synergy_variance = request
        .synergies_achieved
        .minus(&request.synergies_target)
        .round(2);
    let synergy_status = if synergy_rate >= 0.9 {
        "On Track"
    } else if synergy_rate >= 0.7 {
        "Behind"
    } else {
        "Critical"
    };

    let cost_variance = request
        .integration_costs_actual
        .minus(&request.integration_costs_budget)
        .round(2);

    let completed = request
        .milestones
        .iter()
        .filter(|m| m.status == "Completed")
        .count();
    let in_progress = request
        .milestones
        .iter()
        .filter(|m| m.status == "In Progress")
        .count();
    let delayed = request
        .milestones
        .iter()
        .filter(|m| m.status == "Delayed")
        .count();
    let avg_completion = if request.milestones.is_empty() {
        0.0
    } else {
        request
            .milestones
            .iter()
            .map(|m| m.completion_percentage)
            .sum::<f64>()
            / request.milestones.len() as f64
    };

    let mut risk_areas = Vec::new();
    if synergy_rate < 0.8 {
        risk_areas.push("Synergy realization behind schedule".to_string());
    }
    if cost_variance > request.integration_costs_budget.times(&rate(0.1)) {
        risk_areas.push("Integration costs exceeding budget".to_string());
    }
    if delayed as f64 > request.milestones.len() as f64 * 0.3 {
        risk_areas.push("Multiple milestone delays".to_string());
    }

    let overall_status = if risk_areas.is_empty() {
        "Green"
    } else if risk_areas.len() <= 1 {
        "Amber"
    } else {
        "Red"
    };

    let mut recommendations = Vec::new();
    if synergy_rate < 0.9 {
        recommendations
            .push("Accelerate synergy capture - consider additional resources".to_string());
    }
    if delayed > 2 {
        recommendations.push("Review delayed milestones and reallocate resources".to_string());
    }
    if cost_variance > Money::zero() {
        recommendations
            .push("Cost overrun requires immediate attention - review scope".to_string());
    }

    PostMergerOutput {
        deal_id: request.deal_id.clone(),
        status_date: status_date.to_string(),
        synergy_performance: SynergyPerformance {
            target: request.synergies_target.clone(),
            achieved: request.synergies_achieved.clone(),
            rate: r2(synergy_rate * 100.0),
            variance: synergy_variance,
            status: synergy_status.to_string(),
            daily_rate: if request.days_since_close == 0 {
                Money::zero()
            } else {
                request
                    .synergies_achieved
                    .over(&rate(request.days_since_close as f64))
                    .round(2)
            },
        },
        cost_performance: CostPerformance {
            budget: request.integration_costs_budget.clone(),
            actual: request.integration_costs_actual.clone(),
            variance: cost_variance.clone(),
            variance_percentage: if request.integration_costs_budget.is_zero() {
                0.0
            } else {
                r2(cost_variance
                    .over(&request.integration_costs_budget)
                    .to_f64_lossy()
                    * 100.0)
            },
            status: if cost_variance > Money::zero() {
                "Over Budget"
            } else {
                "Under Budget"
            }
            .to_string(),
        },
        integration_progress: IntegrationProgress {
            total_milestones: request.milestones.len(),
            completed,
            in_progress,
            delayed,
            average_completion: r2(avg_completion),
            days_since_close: request.days_since_close,
        },
        risk_areas,
        overall_status: overall_status.to_string(),
        recommendations,
    }
}

// --------------------------------------------------------- synergy analysis

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SynergyItem {
    pub category: String,
    #[serde(default)]
    pub description: String,
    pub annual_value: Money,
    #[serde(default)]
    pub probability: f64,
    #[serde(default)]
    pub timing_months: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SynergyAnalysisRequest {
    pub deal_id: String,
    #[serde(default)]
    pub acquirer_id: String,
    #[serde(default)]
    pub target_id: String,
    #[serde(default)]
    pub synergies: Vec<SynergyItem>,
    pub integration_costs: Money,
    pub cost_of_capital: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SynergySummary {
    pub revenue_synergies: Money,
    pub cost_synergies: Money,
    pub operational_synergies: Money,
    pub other_synergies: Money,
    pub total_annual_synergies: Money,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SynergyAnalysisOutput {
    pub deal_id: String,
    pub synergy_summary: SynergySummary,
    pub pv_synergies: Money,
    pub pv_integration_costs: Money,
    pub net_synergy_value: Money,
    pub irr: f64,
    pub payback_months: f64,
    pub risk_adjusted_value: Money,
    pub recommendations: Vec<String>,
}

pub fn analyze_synergies(request: &SynergyAnalysisRequest) -> SynergyAnalysisOutput {
    let mut revenue = Money::zero();
    let mut cost = Money::zero();
    let mut operational = Money::zero();
    let mut other = Money::zero();

    for item in &request.synergies {
        let weighted = item.annual_value.times(&rate(item.probability)).round(2);
        match item.category.as_str() {
            "revenue" => revenue = revenue.plus(&weighted),
            "cost" => cost = cost.plus(&weighted),
            "operational" => operational = operational.plus(&weighted),
            _ => other = other.plus(&weighted),
        }
    }

    let total_annual = revenue.clone().plus(&cost).plus(&operational).plus(&other);

    // Discounting raises a sum to a fractional power, so each present value is the
    // floating-point figure rounded once to the cent. The undiscounted amounts stay exact.
    let pv_synergies = Money::sum(
        request
            .synergies
            .iter()
            .map(|item| {
                rate(
                    item.annual_value.to_f64_lossy() * item.probability
                        / (1.0 + request.cost_of_capital).powf(item.timing_months / 12.0),
                )
                .round(2)
            })
            .collect::<Vec<Money>>()
            .iter(),
    );

    let total_integration = request.integration_costs.times(&rate(1.1)).round(2);
    let net_value = pv_synergies.minus(&total_integration).round(2);

    let monthly_benefit = total_annual.over(&rate(12.0)).round(2);
    let payback_months = if monthly_benefit.is_zero() {
        0.0
    } else {
        r2(total_integration.over(&monthly_benefit).to_f64_lossy())
    };

    let irr_3yr = if total_integration.is_zero() {
        0.0
    } else {
        ((pv_synergies.over(&total_integration).to_f64_lossy()).powf(1.0 / 3.0) - 1.0) * 100.0
    };

    let risk_adjusted = pv_synergies.times(&rate(0.75)).round(2);

    let mut recommendations = Vec::new();
    if total_annual < request.integration_costs.times(&rate(0.2)) {
        recommendations.push("Synergy case appears weak relative to integration costs".to_string());
    }
    if payback_months > 36.0 {
        recommendations
            .push("Long payback period - ensure board alignment on timeline".to_string());
    }
    if revenue < total_annual.times(&rate(0.2)) {
        recommendations
            .push("Limited revenue synergies - ensure cost synergies are achievable".to_string());
    }
    if net_value.is_negative() {
        recommendations
            .push("WARNING: Negative net synergy value - reconsider deal economics".to_string());
    }

    SynergyAnalysisOutput {
        deal_id: request.deal_id.clone(),
        synergy_summary: SynergySummary {
            revenue_synergies: revenue,
            cost_synergies: cost,
            operational_synergies: operational,
            other_synergies: other,
            total_annual_synergies: total_annual,
        },
        pv_synergies,
        pv_integration_costs: total_integration,
        net_synergy_value: net_value,
        irr: r2(irr_3yr),
        payback_months,
        risk_adjusted_value: risk_adjusted,
        recommendations,
    }
}

// ------------------------------------------------------------- doors

fn parse<T: for<'de> Deserialize<'de>>(payload: &serde_json::Value) -> Result<T, String> {
    serde_json::from_value(payload.clone()).map_err(|error| format!("invalid request: {error}"))
}

fn to_value<T: Serialize>(value: &T) -> Result<serde_json::Value, String> {
    serde_json::to_value(value).map_err(|error| error.to_string())
}

/// `deal-structuring-service /structure`.
pub fn deal_structuring_from_json(
    payload: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    #[derive(Deserialize)]
    struct Request {
        #[serde(flatten)]
        deal: DealStructuringRequest,
        #[serde(default)]
        deal_date: String,
    }
    let request: Request = parse(payload)?;
    to_value(&structure_deal(&request.deal, &request.deal_date))
}

/// `post-merger-service /track`.
pub fn post_merger_from_json(payload: &serde_json::Value) -> Result<serde_json::Value, String> {
    #[derive(Deserialize)]
    struct Request {
        #[serde(flatten)]
        deal: PostMergerRequest,
        #[serde(default)]
        status_date: String,
    }
    let request: Request = parse(payload)?;
    to_value(&track_integration(&request.deal, &request.status_date))
}

/// `synergy-analysis-service /analyze`.
pub fn synergy_analysis_from_json(
    payload: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let request: SynergyAnalysisRequest = parse(payload)?;
    to_value(&analyze_synergies(&request))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(text: &str) -> Money {
        Money::parse(text).expect("amount")
    }

    fn fixed(amount: &Money) -> String {
        amount.to_fixed(2)
    }

    fn deal_request() -> DealStructuringRequest {
        DealStructuringRequest {
            deal_id: "D1".to_string(),
            target_id: "T1".to_string(),
            total_value: m("1000.0"),
            seller_preference: "stock".to_string(),
            tax_efficiency_priority: true,
            earnout_required: true,
            escrow_percentage: 0.05,
            locked_box_date: "2026-01-31".to_string(),
        }
    }

    #[test]
    fn deal_structures_match_the_service() {
        let out = structure_deal(&deal_request(), "2026-11-03T00:00:00");
        assert_eq!(out.structure_options.len(), 3);
        assert_eq!(out.structure_options[0].structure_type, "All Cash");
        assert_eq!(out.structure_options[0].escrow, m("100.0"));
        assert_eq!(out.structure_options[1].upfront_payment, m("800.0"));
        assert_eq!(out.structure_options[1].earnout, m("200.0"));
        assert_eq!(out.structure_options[1].escrow, m("50.0"));
        assert_eq!(out.structure_options[2].deferred_payment, m("200.0"));
        assert_eq!(out.structure_options[2].escrow, m("80.0"));

        assert_eq!(out.recommended_structure.structure_type, "Share Exchange");
        assert_eq!(out.tax_considerations.recommended, "Hybrid");
        assert_eq!(out.risk_allocation.warranty_risk.seller, 0.7);
        assert_eq!(
            out.recommendations,
            vec![
                "Consider share exchange to optimize seller tax position".to_string(),
                "Structure earnout with clear KPIs and caps".to_string(),
                "Consider higher escrow for warranty protection".to_string(),
            ]
        );
        assert_eq!(out.deal_date, "2026-11-03T00:00:00");
    }

    #[test]
    fn seller_preference_picks_the_recommendation() {
        let mut req = deal_request();
        req.seller_preference = "cash".to_string();
        req.tax_efficiency_priority = false;
        let out = structure_deal(&req, "2026-11-03");
        assert_eq!(out.recommended_structure.structure_type, "All Cash");
        assert_eq!(out.tax_considerations.recommended, "Cash");

        req.seller_preference = "anything-else".to_string();
        assert_eq!(
            structure_deal(&req, "2026-11-03")
                .recommended_structure
                .structure_type,
            "Hybrid (Cash + Stock)"
        );
    }

    #[test]
    fn disposal_with_retained_interest_matches_the_service() {
        let out = calculate_disposal(&DivestitureRequest {
            company_id: "C1".to_string(),
            subsidiary_name: "Sub".to_string(),
            carrying_value: 500.0,
            disposal_proceeds: 620.0,
            disposal_costs: 20.0,
            retained_interest_fair_value: 80.0,
            retained_interest_pct: 20.0,
            associated_goodwill: 50.0,
            disposal_method: "sale".to_string(),
        });
        assert_eq!(out.net_proceeds, 600.0);
        assert_eq!(out.gain_or_loss, 150.0);
        assert_eq!(out.total_gain_recognized, 230.0);
        assert_eq!(out.goodwill_derecognized, 50.0);
        assert_eq!(
            out.journal_entries,
            vec![
                "Dr Cash/Bank (600.0)".to_string(),
                "Dr Investment in Subsidiary (retained) (80.0)".to_string(),
                "Cr Investment in Subsidiary (500.0)".to_string(),
                "Cr Gain on Disposal (230.0)".to_string(),
                "Cr Goodwill derecognized".to_string(),
            ]
        );
    }

    #[test]
    fn disposal_at_a_loss_matches_the_service() {
        let out = calculate_disposal(&DivestitureRequest {
            company_id: "C1".to_string(),
            subsidiary_name: "Sub2".to_string(),
            carrying_value: 500.0,
            disposal_proceeds: 400.0,
            disposal_costs: 0.0,
            retained_interest_fair_value: 0.0,
            retained_interest_pct: 0.0,
            associated_goodwill: 0.0,
            disposal_method: "sale".to_string(),
        });
        assert_eq!(out.net_proceeds, 400.0);
        assert_eq!(out.gain_or_loss, -100.0);
        assert_eq!(
            out.journal_entries,
            vec![
                "Dr Cash/Bank (400.0)".to_string(),
                "Cr Investment in Subsidiary (500.0)".to_string(),
                "Dr Loss on Disposal (100.0)".to_string(),
            ]
        );
    }

    #[test]
    fn goodwill_adjustment_line_appears_without_retained_interest() {
        let out = calculate_disposal(&DivestitureRequest {
            company_id: "C1".to_string(),
            subsidiary_name: "Sub3".to_string(),
            carrying_value: 300.0,
            disposal_proceeds: 400.0,
            disposal_costs: 0.0,
            retained_interest_fair_value: 0.0,
            retained_interest_pct: 0.0,
            associated_goodwill: 40.0,
            disposal_method: "spin_off".to_string(),
        });
        // 400 - (300 - 40) = 140
        assert_eq!(out.gain_or_loss, 140.0);
        assert!(out
            .journal_entries
            .iter()
            .any(|e| e == "Dr/Cr Goodwill adjustment"));
        assert_eq!(out.disposal_method, "spin_off");
    }

    fn post_merger_request() -> PostMergerRequest {
        let milestone = |id: &str, status: &str, pct: f64| IntegrationMilestone {
            milestone_id: id.to_string(),
            description: String::new(),
            planned_date: String::new(),
            actual_date: String::new(),
            status: status.to_string(),
            completion_percentage: pct,
        };
        PostMergerRequest {
            deal_id: "D1".to_string(),
            acquirer_id: "A".to_string(),
            target_id: "T".to_string(),
            synergies_achieved: m("75.0"),
            synergies_target: m("100.0"),
            integration_costs_actual: m("55.0"),
            integration_costs_budget: m("40.0"),
            days_since_close: 45,
            milestones: vec![
                milestone("M1", "Completed", 100.0),
                milestone("M2", "In Progress", 50.0),
                milestone("M3", "Delayed", 20.0),
                milestone("M4", "Delayed", 10.0),
            ],
        }
    }

    #[test]
    fn post_merger_tracking_matches_the_service() {
        let out = track_integration(&post_merger_request(), "2026-11-03T00:00:00");
        assert_eq!(out.synergy_performance.rate, 75.0);
        assert_eq!(fixed(&out.synergy_performance.variance), "-25.00");
        assert_eq!(out.synergy_performance.status, "Behind");
        assert_eq!(out.synergy_performance.daily_rate, m("1.67"));
        assert_eq!(fixed(&out.cost_performance.variance), "15.00");
        assert_eq!(out.cost_performance.variance_percentage, 37.5);
        assert_eq!(out.cost_performance.status, "Over Budget");
        assert_eq!(out.integration_progress.completed, 1);
        assert_eq!(out.integration_progress.in_progress, 1);
        assert_eq!(out.integration_progress.delayed, 2);
        assert_eq!(out.integration_progress.average_completion, 45.0);
        assert_eq!(out.risk_areas.len(), 3);
        assert_eq!(out.overall_status, "Red");
        assert_eq!(
            out.recommendations,
            vec![
                "Accelerate synergy capture - consider additional resources".to_string(),
                "Cost overrun requires immediate attention - review scope".to_string(),
            ]
        );
    }

    #[test]
    fn healthy_integration_is_green() {
        let mut req = post_merger_request();
        req.synergies_achieved = m("95.0");
        req.integration_costs_actual = m("30.0");
        req.milestones = vec![IntegrationMilestone {
            milestone_id: "M1".to_string(),
            description: String::new(),
            planned_date: String::new(),
            actual_date: String::new(),
            status: "Completed".to_string(),
            completion_percentage: 100.0,
        }];
        let out = track_integration(&req, "2026-11-03");
        assert!(out.risk_areas.is_empty());
        assert_eq!(out.overall_status, "Green");
        assert_eq!(out.cost_performance.status, "Under Budget");
        assert!(out.recommendations.is_empty());
    }

    fn synergy_request() -> SynergyAnalysisRequest {
        let item = |category: &str, value: &str, probability: f64, months: f64| SynergyItem {
            category: category.to_string(),
            description: String::new(),
            annual_value: m(value),
            probability,
            timing_months: months,
        };
        SynergyAnalysisRequest {
            deal_id: "D1".to_string(),
            acquirer_id: "A".to_string(),
            target_id: "T".to_string(),
            synergies: vec![
                item("revenue", "120.0", 0.6, 12.0),
                item("cost", "200.0", 0.8, 6.0),
                item("operational", "50.0", 0.5, 24.0),
                item("other", "30.0", 1.0, 0.0),
            ],
            integration_costs: m("150.0"),
            cost_of_capital: 0.09,
        }
    }

    #[test]
    fn synergy_analysis_matches_the_service() {
        let out = analyze_synergies(&synergy_request());
        assert_eq!(out.synergy_summary.revenue_synergies, m("72.0"));
        assert_eq!(out.synergy_summary.cost_synergies, m("160.0"));
        assert_eq!(out.synergy_summary.operational_synergies, m("25.0"));
        assert_eq!(out.synergy_summary.other_synergies, m("30.0"));
        assert_eq!(out.synergy_summary.total_annual_synergies, m("287.0"));
        assert_eq!(out.pv_synergies, m("270.35"));
        assert_eq!(out.pv_integration_costs, m("165.0"));
        assert_eq!(out.net_synergy_value, m("105.35"));
        assert_eq!(out.irr, 17.89);
        assert!((out.payback_months - 6.9).abs() < 1e-9);
        assert_eq!(out.risk_adjusted_value, m("202.76"));
        assert!(out.recommendations.is_empty());
    }

    #[test]
    fn weak_synergy_case_raises_every_warning() {
        let mut req = synergy_request();
        req.synergies = vec![SynergyItem {
            category: "cost".to_string(),
            description: String::new(),
            annual_value: m("5.0"),
            probability: 1.0,
            timing_months: 0.0,
        }];
        req.integration_costs = m("100.0");
        let out = analyze_synergies(&req);
        assert!(out.net_synergy_value.is_negative());
        assert_eq!(out.recommendations.len(), 4);
        assert!(out.recommendations[3].starts_with("WARNING: Negative net synergy value"));
    }

    #[test]
    fn py_float_str_keeps_python_float_style() {
        assert_eq!(py_float_str(600.0), "600.0");
        assert_eq!(py_float_str(-100.0), "-100.0");
        assert_eq!(py_float_str(12.5), "12.5");
    }

    #[test]
    fn zero_denominators_do_not_divide() {
        let mut req = synergy_request();
        req.integration_costs = m("0");
        assert_eq!(analyze_synergies(&req).irr, 0.0);

        let mut pm = post_merger_request();
        pm.days_since_close = 0;
        pm.synergies_target = m("0");
        pm.integration_costs_budget = m("0");
        let out = track_integration(&pm, "2026-11-03");
        assert_eq!(fixed(&out.synergy_performance.daily_rate), "0.00");
        assert_eq!(out.synergy_performance.rate, 0.0);
        assert_eq!(out.cost_performance.variance_percentage, 0.0);
    }
}
