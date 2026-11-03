#![forbid(unsafe_code)]
//! Project and investment appraisal, ported from project-appraisal-service and
//! investment-appraisal-service.
//!
//! The service used two ladders rather than real root finders, and both are
//! kept:
//!
//! * IRR scans 1%..100% in 1% steps and takes the first rate whose NPV is not
//!   positive. If every rate leaves NPV positive the figure stays at 0, which
//!   is what the service reported - it does not report "none".
//! * Payback interpolates as `i + (initial - sum(flows before i)) / flow_at_i`
//!   using the zero-based index, and reports 1.0 when the investment is
//!   recovered in the first year, so the number is a year count, not an index.
//!
//! One Python crash is turned into a value: adding a salvage value to an empty
//! cash-flow list raised `IndexError` in the service, so [`project_appraisal`]
//! returns that flows were missing instead of panicking.

use serde::{Deserialize, Serialize};
use vimbai_common::py_round;

fn r2(value: f64) -> f64 {
    py_round(value, 2)
}

fn r3(value: f64) -> f64 {
    py_round(value, 3)
}

fn r4(value: f64) -> f64 {
    py_round(value, 4)
}

fn r1(value: f64) -> f64 {
    py_round(value, 1)
}

// ------------------------------------------------------------ project level

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AppraisalRequest {
    pub company_id: String,
    pub project_name: String,
    pub initial_investment: f64,
    pub cash_flows: Vec<f64>,
    pub discount_rate: f64,
    pub salvage_value: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CashFlowAnalysis {
    pub year: usize,
    pub cash_flow: f64,
    pub discounted: f64,
    pub cumulative: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProjectAppraisalOutput {
    pub company_id: String,
    pub project_name: String,
    pub npv: f64,
    pub irr: f64,
    pub payback_period: f64,
    pub discounted_payback: f64,
    pub profitability_index: f64,
    pub recommendation: String,
    pub cash_flow_analysis: Vec<CashFlowAnalysis>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl Default for AppraisalRequest {
    fn default() -> Self {
        Self {
            company_id: String::new(),
            project_name: String::new(),
            initial_investment: 0.0,
            cash_flows: Vec::new(),
            discount_rate: 0.10,
            salvage_value: 0.0,
        }
    }
}

pub fn project_appraisal(request: &AppraisalRequest) -> ProjectAppraisalOutput {
    let mut flows = request.cash_flows.clone();
    if request.salvage_value != 0.0 {
        match flows.last_mut() {
            Some(last) => *last += request.salvage_value,
            None => {
                return ProjectAppraisalOutput {
                    company_id: request.company_id.clone(),
                    project_name: request.project_name.clone(),
                    npv: 0.0,
                    irr: 0.0,
                    payback_period: 0.0,
                    discounted_payback: 0.0,
                    profitability_index: 0.0,
                    recommendation: "Reject: no cash flows to appraise".to_string(),
                    cash_flow_analysis: Vec::new(),
                    error: Some("salvage value supplied without cash flows".to_string()),
                }
            }
        }
    }

    let mut npv = -request.initial_investment;
    for (index, cash_flow) in flows.iter().enumerate() {
        npv += cash_flow / (1.0 + request.discount_rate).powi((index + 1) as i32);
    }

    let mut irr = 0.0;
    for step in 1..=100 {
        let rate = step as f64 * 0.01;
        let mut test = -request.initial_investment;
        for (index, cash_flow) in flows.iter().enumerate() {
            test += cash_flow / (1.0 + rate).powi((index + 1) as i32);
        }
        if test <= 0.0 {
            irr = rate;
            break;
        }
    }

    let mut cumulative = -request.initial_investment;
    let mut payback = 0.0;
    for (index, cash_flow) in flows.iter().enumerate() {
        cumulative += cash_flow;
        if cumulative >= 0.0 {
            payback = if index > 0 {
                let inflow_before: f64 = flows[..index].iter().sum();
                index as f64 + (request.initial_investment - inflow_before) / cash_flow
            } else {
                1.0
            };
            break;
        }
    }

    let mut discounted_cumulative = -request.initial_investment;
    let mut discounted_payback = 0.0;
    for (index, cash_flow) in flows.iter().enumerate() {
        discounted_cumulative += cash_flow / (1.0 + request.discount_rate).powi((index + 1) as i32);
        if discounted_cumulative >= 0.0 {
            discounted_payback = (index + 1) as f64;
            break;
        }
    }

    let profitability_index = if request.initial_investment == 0.0 {
        0.0
    } else {
        (npv + request.initial_investment) / request.initial_investment
    };

    let mut analysis = Vec::with_capacity(flows.len());
    let mut running = -request.initial_investment;
    for (index, cash_flow) in flows.iter().enumerate() {
        let discounted = cash_flow / (1.0 + request.discount_rate).powi((index + 1) as i32);
        running += cash_flow;
        analysis.push(CashFlowAnalysis {
            year: index + 1,
            cash_flow: r2(*cash_flow),
            discounted: r2(discounted),
            cumulative: r2(running),
        });
    }

    let recommendation = if npv > 0.0 && irr > request.discount_rate {
        format!(
            "Accept: NPV positive ({:.0}), IRR ({:.1}%) exceeds discount rate",
            npv,
            irr * 100.0
        )
    } else {
        "Reject: NPV negative or IRR below discount rate".to_string()
    };

    ProjectAppraisalOutput {
        company_id: request.company_id.clone(),
        project_name: request.project_name.clone(),
        npv: r2(npv),
        irr: r1(irr * 100.0),
        payback_period: r1(payback),
        discounted_payback: r1(discounted_payback),
        profitability_index: r3(profitability_index),
        recommendation,
        cash_flow_analysis: analysis,
        error: None,
    }
}

// --------------------------------------------------------- portfolio level

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScoredProjectInput {
    pub id: String,
    pub npv: f64,
    pub strategic_value: f64,
    pub risk_level: f64,
    pub environmental_impact: f64,
    pub social_impact: f64,
    pub governance_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AppraisalWeights {
    pub financial: f64,
    pub strategic: f64,
    pub risk: f64,
    pub esg: f64,
}

impl Default for AppraisalWeights {
    fn default() -> Self {
        Self {
            financial: 0.4,
            strategic: 0.2,
            risk: 0.2,
            esg: 0.2,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AppraisalResult {
    pub project_id: String,
    pub financial_score: f64,
    pub strategic_score: f64,
    pub risk_score: f64,
    pub environmental_score: f64,
    pub social_score: f64,
    pub governance_score: f64,
    pub overall_score: f64,
    pub recommendation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InvestmentAppraisalOutput {
    pub company_id: String,
    pub appraisal_results: Vec<AppraisalResult>,
    pub best_project: String,
    pub recommended_projects: Vec<String>,
    pub recommendations: Vec<String>,
}

pub fn appraise_investments(
    company_id: &str,
    projects: &[ScoredProjectInput],
    weights: &AppraisalWeights,
) -> InvestmentAppraisalOutput {
    let results: Vec<AppraisalResult> = projects
        .iter()
        .map(|project| {
            let financial = project.npv / 1_000_000.0;
            let strategic = project.strategic_value / 10.0;
            let risk = 1.0 - project.risk_level;
            let environmental = project.environmental_impact / 10.0;
            let social = project.social_impact / 10.0;
            let governance = project.governance_score / 10.0;
            let overall = financial * weights.financial
                + strategic * weights.strategic
                + risk * weights.risk
                + (environmental + social + governance) / 3.0 * weights.esg;
            AppraisalResult {
                project_id: project.id.clone(),
                financial_score: r2(financial),
                strategic_score: r2(strategic),
                risk_score: r2(risk),
                environmental_score: r2(environmental),
                social_score: r2(social),
                governance_score: r2(governance),
                overall_score: r4(overall),
                recommendation: if overall > 0.6 {
                    "approve".to_string()
                } else if overall > 0.4 {
                    "review".to_string()
                } else {
                    "reject".to_string()
                },
            }
        })
        .collect();

    let mut ranked = results.clone();
    ranked.sort_by(|a, b| {
        b.overall_score
            .partial_cmp(&a.overall_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let best_project = ranked
        .first()
        .map(|r| r.project_id.clone())
        .unwrap_or_default();
    let recommended_projects: Vec<String> = ranked
        .iter()
        .filter(|r| r.recommendation == "approve")
        .take(3)
        .map(|r| r.project_id.clone())
        .collect();

    let recommendations = if recommended_projects.is_empty() {
        vec!["No projects meet the approval threshold".to_string()]
    } else {
        vec![
            "Prioritize highest scoring investments".to_string(),
            "Review medium-scored projects for improvement".to_string(),
            "Consider portfolio diversification".to_string(),
        ]
    };

    InvestmentAppraisalOutput {
        company_id: company_id.to_string(),
        appraisal_results: results,
        best_project,
        recommended_projects,
        recommendations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_request() -> AppraisalRequest {
        AppraisalRequest {
            company_id: "c1".into(),
            project_name: "Dig".into(),
            initial_investment: 400_000.0,
            cash_flows: vec![150_000.0, 150_000.0, 150_000.0, 150_000.0],
            discount_rate: 0.10,
            salvage_value: 0.0,
        }
    }

    #[test]
    fn project_appraisal_matches_service() {
        let out = project_appraisal(&base_request());
        assert_eq!(out.npv, 75_479.82);
        assert_eq!(out.irr, 19.0);
        assert_eq!(out.payback_period, 2.7);
        assert_eq!(out.discounted_payback, 4.0);
        assert_eq!(out.profitability_index, 1.189);
        assert!(out.recommendation.starts_with("Accept: NPV positive"));
        assert_eq!(out.cash_flow_analysis.len(), 4);
        assert_eq!(out.cash_flow_analysis[0].discounted, 136_363.64);
        assert!(out.error.is_none());
    }

    #[test]
    fn salvage_lands_on_the_final_flow() {
        let mut request = base_request();
        request.salvage_value = 50_000.0;
        let out = project_appraisal(&request);
        assert_eq!(out.cash_flow_analysis[3].cash_flow, 200_000.0);
    }

    #[test]
    fn salvage_without_flows_is_an_error_not_a_panic() {
        let mut request = base_request();
        request.cash_flows.clear();
        request.salvage_value = 10_000.0;
        let out = project_appraisal(&request);
        assert!(out.error.is_some());
        assert_eq!(out.recommendation, "Reject: no cash flows to appraise");
    }

    #[test]
    fn weak_project_is_rejected() {
        let request = AppraisalRequest {
            company_id: "c1".into(),
            project_name: "Thin".into(),
            initial_investment: 500_000.0,
            cash_flows: vec![120_000.0, 120_000.0],
            discount_rate: 0.10,
            salvage_value: 0.0,
        };
        let out = project_appraisal(&request);
        assert!(out.npv < 0.0);
        assert_eq!(
            out.recommendation,
            "Reject: NPV negative or IRR below discount rate"
        );
    }

    #[test]
    fn investment_scoring_ranks_and_approves() {
        let projects = vec![
            ScoredProjectInput {
                id: "p1".into(),
                npv: 900_000.0,
                strategic_value: 9.0,
                risk_level: 0.2,
                environmental_impact: 8.0,
                social_impact: 8.0,
                governance_score: 9.0,
            },
            ScoredProjectInput {
                id: "p2".into(),
                npv: 100_000.0,
                strategic_value: 4.0,
                risk_level: 0.8,
                environmental_impact: 4.0,
                social_impact: 5.0,
                governance_score: 4.0,
            },
        ];
        let out = appraise_investments("c1", &projects, &AppraisalWeights::default());
        assert_eq!(out.best_project, "p1");
        assert_eq!(out.recommended_projects, vec!["p1".to_string()]);
        assert_eq!(out.appraisal_results[0].overall_score, 0.8667);
        assert_eq!(out.appraisal_results[1].recommendation, "reject");
    }

    #[test]
    fn no_approvals_gets_its_own_recommendation() {
        let projects = vec![ScoredProjectInput {
            id: "p3".into(),
            npv: 10_000.0,
            strategic_value: 2.0,
            risk_level: 0.9,
            environmental_impact: 2.0,
            social_impact: 2.0,
            governance_score: 2.0,
        }];
        let out = appraise_investments("c1", &projects, &AppraisalWeights::default());
        assert!(out.recommended_projects.is_empty());
        assert_eq!(
            out.recommendations[0],
            "No projects meet the approval threshold"
        );
    }
}
