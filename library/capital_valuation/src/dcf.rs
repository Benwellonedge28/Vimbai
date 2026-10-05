#![forbid(unsafe_code)]
//! Discounted cash flow primitives: NPV, discount factors, annuity
//! factors, project comparison and profitability index.
//!
//! Ported from net-present-value-service and discount-factor-service.

use serde::{Deserialize, Serialize};
use vimbai_common::{py_round, py_round2};

/// Decision emitted by the NPV services.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NpvDecision {
    #[serde(rename = "Accept - Positive NPV")]
    AcceptPositive,
    #[serde(rename = "Reject - Negative NPV")]
    RejectNegative,
    #[serde(rename = "Indifferent - Zero NPV")]
    Indifferent,
}

impl NpvDecision {
    fn from_npv(npv: f64) -> Self {
        if npv > 0.0 {
            Self::AcceptPositive
        } else if npv < 0.0 {
            Self::RejectNegative
        } else {
            Self::Indifferent
        }
    }
}

/// Per-year (or residual) present value line item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NpvDetail {
    pub year: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cash_flow: Option<f64>,
    pub discount_factor: f64,
    pub present_value: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NpvOutput {
    pub initial_investment: f64,
    pub discount_rate: f64,
    pub residual_value: f64,
    pub cash_flow_details: Vec<NpvDetail>,
    pub total_pv_inflows: f64,
    pub net_present_value: f64,
    pub decision: NpvDecision,
}

/// Net Present Value (net-present-value-service `/calculate`).
///
/// `discount_rate` is a decimal (10% = 0.10).
pub fn npv(
    initial_investment: f64,
    cash_flows: &[f64],
    discount_rate: f64,
    residual_value: f64,
) -> NpvOutput {
    let mut pv_inflows = 0.0;
    let mut details = Vec::with_capacity(cash_flows.len());

    for (i, cf) in cash_flows.iter().enumerate() {
        let year = i + 1;
        let df = 1.0 / (1.0 + discount_rate).powi(year as i32);
        let pv = cf * df;
        pv_inflows += pv;
        details.push(NpvDetail {
            year,
            description: None,
            cash_flow: Some(*cf),
            discount_factor: py_round(df, 4),
            present_value: py_round2(pv),
        });
    }

    if residual_value > 0.0 {
        let years = cash_flows.len();
        let df = 1.0 / (1.0 + discount_rate).powi(years as i32);
        let pv_residual = residual_value * df;
        pv_inflows += pv_residual;
        details.push(NpvDetail {
            year: years,
            description: Some("Residual Value".to_string()),
            cash_flow: Some(residual_value),
            discount_factor: py_round(df, 4),
            present_value: py_round2(pv_residual),
        });
    }

    let net = pv_inflows - initial_investment;

    NpvOutput {
        initial_investment,
        discount_rate,
        residual_value,
        cash_flow_details: details,
        total_pv_inflows: py_round2(pv_inflows),
        net_present_value: py_round2(net),
        decision: NpvDecision::from_npv(net),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NpvPercentOutput {
    pub initial_investment: f64,
    pub discount_rate_percent: f64,
    pub discount_rate_decimal: f64,
    pub residual_value: f64,
    pub net_present_value: f64,
    pub decision: String,
}

/// NPV with the rate given as a percentage
/// (net-present-value-service `/calculate-percent`).
pub fn npv_percent(
    initial_investment: f64,
    cash_flows: &[f64],
    discount_rate_percent: f64,
    residual_value: f64,
) -> NpvPercentOutput {
    let rate = discount_rate_percent / 100.0;
    let out = npv(initial_investment, cash_flows, rate, residual_value);
    let decision = if out.net_present_value > 0.0 {
        "Accept"
    } else if out.net_present_value < 0.0 {
        "Reject"
    } else {
        "Indifferent"
    };
    NpvPercentOutput {
        initial_investment,
        discount_rate_percent,
        discount_rate_decimal: rate,
        residual_value,
        net_present_value: out.net_present_value,
        decision: decision.to_string(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectNpv {
    pub name: String,
    pub initial_investment: f64,
    pub net_present_value: f64,
}

/// Compare NPV across projects (net-present-value-service `/compare`).
/// Returns projects ranked best (highest NPV) first.
pub fn npv_project_comparison(
    projects: &[(String, f64, Vec<f64>)],
    discount_rate: f64,
) -> Vec<ProjectNpv> {
    let mut results: Vec<ProjectNpv> = projects
        .iter()
        .map(|(name, inv, flows)| {
            let out = npv(*inv, flows, discount_rate, 0.0);
            ProjectNpv {
                name: name.clone(),
                initial_investment: *inv,
                net_present_value: out.net_present_value,
            }
        })
        .collect();
    results.sort_by(|a, b| {
        b.net_present_value
            .partial_cmp(&a.net_present_value)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    results
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfitabilityIndexOutput {
    pub initial_investment: f64,
    pub total_pv_inflows: f64,
    pub profitability_index: f64,
    pub decision: String,
}

/// Profitability Index = PV of inflows / initial investment
/// (net-present-value-service `/profitability-index`).
pub fn profitability_index(
    initial_investment: f64,
    cash_flows: &[f64],
    discount_rate: f64,
    residual_value: f64,
) -> ProfitabilityIndexOutput {
    let out = npv(
        initial_investment,
        cash_flows,
        discount_rate,
        residual_value,
    );
    let index = if initial_investment != 0.0 {
        out.total_pv_inflows / initial_investment
    } else {
        f64::NAN
    };
    ProfitabilityIndexOutput {
        initial_investment,
        total_pv_inflows: out.total_pv_inflows,
        profitability_index: py_round2(index),
        decision: if index >= 1.0 { "Accept" } else { "Reject" }.to_string(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscountFactorResult {
    pub rate: f64,
    pub years: u32,
    pub discount_factor: f64,
}

/// Discount factor `DF = 1 / (1 + r)^n` (discount-factor-service).
pub fn discount_factor(rate: f64, years: u32) -> DiscountFactorResult {
    DiscountFactorResult {
        rate,
        years,
        discount_factor: 1.0 / (1.0 + rate).powi(years as i32),
    }
}

/// Discount factor from a percentage rate.
pub fn discount_factor_percent(rate_percent: f64, years: u32) -> DiscountFactorResult {
    discount_factor(rate_percent / 100.0, years)
}

/// Present-value annuity factor `PVAF = [1 - 1/(1+r)^n] / r`
/// (rate 0 => n).
pub fn annuity_factor(rate: f64, years: u32) -> f64 {
    if rate == 0.0 {
        years as f64
    } else {
        (1.0 - 1.0 / (1.0 + rate).powi(years as i32)) / rate
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscountFactorEntry {
    pub rate: f64,
    pub years: u32,
    pub discount_factor: f64,
}

/// Discount factor table for one rate over several horizons.
pub fn discount_factor_table(base_rate: f64, years: &[u32]) -> Vec<DiscountFactorEntry> {
    years
        .iter()
        .map(|&y| DiscountFactorEntry {
            rate: base_rate,
            years: y,
            discount_factor: discount_factor(base_rate, y).discount_factor,
        })
        .collect()
}

/// Discount factor table across a rate range (inclusive, fixed step).
pub fn discount_factor_table_range(
    rate_start: f64,
    rate_end: f64,
    rate_step: f64,
    years: u32,
) -> Vec<DiscountFactorEntry> {
    let mut entries = Vec::new();
    let mut rate = rate_start;
    let mut guard = 0;
    while rate <= rate_end + 1e-12 && guard < 10_000 {
        entries.push(DiscountFactorEntry {
            rate: py_round(rate, 6),
            years,
            discount_factor: discount_factor(rate, years).discount_factor,
        });
        rate += rate_step;
        guard += 1;
    }
    entries
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn npv_matches_python_service() {
        // python: inv=10000, flows=[3000,4200,6800], rate=0.10, residual=1000
        let out = npv(10000.0, &[3000.0, 4200.0, 6800.0], 0.10, 1000.0);
        // df1=0.9091 pv=2727.27; df2=0.8264 pv=3471.07; df3=0.7513 pv=5108.94
        // residual: 1000*0.7513=751.31 -> pv_inflows=12058.60 npv=2058.60
        assert!((out.net_present_value - 2058.6).abs() < 0.01);
        assert_eq!(out.decision, NpvDecision::AcceptPositive);
        assert_eq!(out.cash_flow_details.len(), 4);
        assert_eq!(
            out.cash_flow_details[3].description.as_deref(),
            Some("Residual Value")
        );
    }

    #[test]
    fn npv_rejects_negative() {
        let out = npv(1000.0, &[100.0, 100.0], 0.50, 0.0);
        assert_eq!(out.decision, NpvDecision::RejectNegative);
    }

    #[test]
    fn npv_percent_converts_rate() {
        let out = npv_percent(1000.0, &[600.0, 600.0], 10.0, 0.0);
        assert!((out.discount_rate_decimal - 0.10).abs() < 1e-12);
        assert_eq!(out.decision, "Accept");
    }

    #[test]
    fn comparison_ranks_best_first() {
        let projects = vec![
            ("A".into(), 1000.0, vec![100.0, 100.0]),
            ("B".into(), 1000.0, vec![800.0, 800.0]),
        ];
        let ranked = npv_project_comparison(&projects, 0.10);
        assert_eq!(ranked[0].name, "B");
    }

    #[test]
    fn discount_factor_basics() {
        let df = discount_factor(0.10, 1);
        assert!((df.discount_factor - 0.909090909).abs() < 1e-9);
        let dfp = discount_factor_percent(10.0, 2);
        assert!((dfp.discount_factor - 0.826446281).abs() < 1e-9);
    }

    #[test]
    fn annuity_factor_matches_formula() {
        // [1 - 1/(1.1)^3]/0.1 = 2.48685...
        assert!((annuity_factor(0.10, 3) - 2.486851991).abs() < 1e-9);
        assert_eq!(annuity_factor(0.0, 5), 5.0);
    }

    #[test]
    fn table_range_spans_rates() {
        let t = discount_factor_table_range(0.05, 0.15, 0.05, 5);
        assert_eq!(t.len(), 3);
        assert!((t[0].rate - 0.05).abs() < 1e-12);
    }

    #[test]
    fn profitability_index_decision() {
        let out = profitability_index(1000.0, &[600.0, 600.0], 0.0, 0.0);
        assert!((out.profitability_index - 1.2).abs() < 1e-9);
        assert_eq!(out.decision, "Accept");
    }
}
