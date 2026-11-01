#![forbid(unsafe_code)]
//! Core budgeting: prepare / monitor / variance / forecast,
//! ported from budget-service with exact formula parity.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use vimbai_common::py_round2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrepareResult {
    pub company_id: String,
    pub revenue: f64,
    pub cogs: f64,
    pub gross_profit: f64,
    pub opex: f64,
    pub ebitda: f64,
}

/// High-level budget summary (budget-service `/prepare`).
pub fn prepare_budget(company_id: &str, revenue: f64, cogs: f64, opex: f64) -> PrepareResult {
    let gross_profit = revenue - cogs;
    PrepareResult {
        company_id: company_id.to_string(),
        revenue,
        cogs,
        gross_profit: py_round2(gross_profit),
        opex,
        ebitda: py_round2(gross_profit - opex),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorResult {
    pub company_id: String,
    pub budget: f64,
    pub actual: f64,
    pub variance: f64,
    pub variance_pct: f64,
    pub status: String,
}

/// Single budget line monitor (budget-service `/monitor`).
/// `on_track` when |pct| < 5, else `attention_needed`.
pub fn monitor_budget(company_id: &str, budget: f64, actual: f64) -> MonitorResult {
    let variance = actual - budget;
    let pct = if budget != 0.0 {
        variance / budget * 100.0
    } else {
        0.0
    };
    MonitorResult {
        company_id: company_id.to_string(),
        budget,
        actual,
        variance: py_round2(variance),
        variance_pct: py_round2(pct),
        status: if pct.abs() < 5.0 {
            "on_track".to_string()
        } else {
            "attention_needed".to_string()
        },
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetItem {
    pub item_id: String,
    pub item_name: String,
    pub category: String,
    pub budget: f64,
    pub actual: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VarianceItemAnalysis {
    pub item_id: String,
    pub item_name: String,
    pub category: String,
    pub budget: f64,
    pub actual: f64,
    pub variance: f64,
    pub variance_pct: f64,
    /// favorable = under budget (variance < 0)
    pub favorable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignificantVariance {
    pub item: String,
    pub variance: f64,
    pub variance_pct: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VarianceSummary {
    pub total_budget: f64,
    pub total_actual: f64,
    pub total_variance: f64,
    pub variance_pct: f64,
    pub cost_center: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetVarianceRequest {
    pub company_id: String,
    pub period: String,
    pub items: Vec<BudgetItem>,
    #[serde(default)]
    pub cost_center: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetVarianceResult {
    pub summary: VarianceSummary,
    pub item_analysis: Vec<VarianceItemAnalysis>,
    /// Items whose |variance_pct| exceeds 10%.
    pub significant_variances: Vec<SignificantVariance>,
    pub recommendations: Vec<String>,
}

/// Item-level variance analysis (budget-service `/analyze`).
pub fn analyze_budget_variance(req: &BudgetVarianceRequest) -> BudgetVarianceResult {
    let mut item_analysis = Vec::with_capacity(req.items.len());
    let mut significant = Vec::new();
    let mut total_budget = 0.0;
    let mut total_actual = 0.0;
    for item in &req.items {
        let variance = item.actual - item.budget;
        let variance_pct = if item.budget != 0.0 {
            variance / item.budget * 100.0
        } else {
            0.0
        };
        item_analysis.push(VarianceItemAnalysis {
            item_id: item.item_id.clone(),
            item_name: item.item_name.clone(),
            category: item.category.clone(),
            budget: py_round2(item.budget),
            actual: py_round2(item.actual),
            variance: py_round2(variance),
            variance_pct: py_round2(variance_pct),
            favorable: variance < 0.0,
        });
        total_budget += item.budget;
        total_actual += item.actual;
        if variance_pct.abs() > 10.0 {
            significant.push(SignificantVariance {
                item: item.item_name.clone(),
                variance: py_round2(variance),
                variance_pct: py_round2(variance_pct),
            });
        }
    }
    let total_variance = total_actual - total_budget;
    let total_variance_pct = if total_budget != 0.0 {
        total_variance / total_budget * 100.0
    } else {
        0.0
    };
    let mut recommendations = Vec::new();
    if total_variance_pct.abs() > 5.0 {
        recommendations
            .push("Overall budget variance exceeds 5% — investigate drivers".to_string());
    }
    if significant.len() > 5 {
        recommendations.push("Multiple significant variances — review exception items".to_string());
    }
    BudgetVarianceResult {
        summary: VarianceSummary {
            total_budget: py_round2(total_budget),
            total_actual: py_round2(total_actual),
            total_variance: py_round2(total_variance),
            variance_pct: py_round2(total_variance_pct),
            cost_center: req.cost_center.clone(),
        },
        item_analysis,
        significant_variances: significant,
        recommendations,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActualResult {
    pub account_id: Option<String>,
    pub description: String,
    pub budget: f64,
    pub actual: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutsideTolerance {
    pub account_id: Option<String>,
    pub description: String,
    pub budget: f64,
    pub actual: f64,
    pub variance: f64,
    pub variance_pct: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetailedVarianceResult {
    pub company_id: String,
    pub budget_id: String,
    pub total_variance: f64,
    pub favorable_variance: f64,
    pub unfavorable_variance: f64,
    pub items_outside_tolerance: Vec<OutsideTolerance>,
    pub corrective_actions: Vec<String>,
}

/// Favorable/unfavorable split (budget-service `/variance-analysis`).
/// Favorable accumulates absolute under-spend; unfavorable accumulates
/// over-spend. `total_variance = unfavorable - favorable`.
pub fn detailed_variance_analysis(
    company_id: &str,
    budget_id: &str,
    actual_results: &[ActualResult],
    tolerance_percentage: f64,
) -> DetailedVarianceResult {
    let mut favorable = 0.0;
    let mut unfavorable = 0.0;
    let mut outside = Vec::new();
    for item in actual_results {
        let variance = item.actual - item.budget;
        let variance_pct = if item.budget != 0.0 {
            variance / item.budget * 100.0
        } else {
            0.0
        };
        if variance_pct.abs() > tolerance_percentage {
            outside.push(OutsideTolerance {
                account_id: item.account_id.clone(),
                description: item.description.clone(),
                budget: item.budget,
                actual: item.actual,
                variance: py_round2(variance),
                variance_pct: py_round2(variance_pct),
            });
        }
        if variance < 0.0 {
            favorable += variance.abs();
        } else {
            unfavorable += variance;
        }
    }
    DetailedVarianceResult {
        company_id: company_id.to_string(),
        budget_id: budget_id.to_string(),
        total_variance: py_round2(unfavorable - favorable),
        favorable_variance: py_round2(favorable),
        unfavorable_variance: py_round2(unfavorable),
        items_outside_tolerance: outside,
        corrective_actions: vec![
            "Review pricing strategy".to_string(),
            "Renegotiate supplier contracts".to_string(),
            "Implement cost controls".to_string(),
        ],
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForecastRequest {
    pub company_id: String,
    pub periods: i64,
    #[serde(default = "default_method")]
    pub forecast_method: String,
    pub historical_data: Vec<f64>,
    #[serde(default)]
    pub growth_rates: BTreeMap<String, f64>,
    #[serde(default)]
    pub seasonal_factors: BTreeMap<String, f64>,
}

fn default_method() -> String {
    "growth_rate".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Projection {
    pub period: i64,
    pub projected_value: f64,
    pub growth_rate: f64,
    pub seasonal_factor: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfidenceInterval {
    pub period: i64,
    pub lower: f64,
    pub upper: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForecastResult {
    pub company_id: String,
    pub forecast_method: String,
    pub periods_forecast: i64,
    pub projections: Vec<Projection>,
    pub confidence_intervals: Vec<ConfidenceInterval>,
    pub accuracy_score: f64,
}

/// Growth-rate forecast (budget-service `/forecast`). Default growth
/// 3%; quarters `Q1..Q4` selected by `(i-1)//3 + 1`; default factor 1.0;
/// confidence band ±10%.
pub fn generate_forecast(req: &ForecastRequest) -> ForecastResult {
    let base_value: f64 = req.historical_data.iter().sum();
    let growth = req.growth_rates.get("default").copied().unwrap_or(0.03);
    let mut projections = Vec::new();
    let mut intervals = Vec::new();
    for i in 1..=req.periods {
        let quarter = (i - 1) / 3 + 1;
        let seasonal = req
            .seasonal_factors
            .get(&format!("Q{quarter}"))
            .copied()
            .unwrap_or(1.0);
        let projected = base_value * (1.0 + growth).powi(i as i32) * seasonal;
        let rounded = py_round2(projected);
        projections.push(Projection {
            period: i,
            projected_value: rounded,
            growth_rate: growth,
            seasonal_factor: seasonal,
        });
        intervals.push(ConfidenceInterval {
            period: i,
            lower: py_round2(rounded * 0.9),
            upper: py_round2(rounded * 1.1),
        });
    }
    ForecastResult {
        company_id: req.company_id.clone(),
        forecast_method: req.forecast_method.clone(),
        periods_forecast: req.periods,
        projections,
        confidence_intervals: intervals,
        accuracy_score: 0.92,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioRow {
    pub period: i64,
    pub revenue: f64,
    pub expenses: f64,
    pub profit: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollingForecastResult {
    pub company_id: String,
    pub base_actual: f64,
    pub forward_projections: Vec<ScenarioRow>,
    pub optimistic: Vec<ScenarioRow>,
    pub pessimistic: Vec<ScenarioRow>,
    pub recommended_scenario: String,
}

/// Rolling forecast (budget-service `/rolling-forecast`). The Python
/// body is a fixed base-1M model with 5%/3%/7% growth — ported
/// verbatim, including its independence from `base_periods`.
pub fn rolling_forecast(company_id: &str, forward_periods: i64) -> RollingForecastResult {
    let mut forward = Vec::new();
    for i in 1..=forward_periods {
        forward.push(ScenarioRow {
            period: i,
            revenue: py_round2(1_000_000.0 * 1.05f64.powi(i as i32)),
            expenses: py_round2(700_000.0 * 1.03f64.powi(i as i32)),
            profit: py_round2(300_000.0 * 1.07f64.powi(i as i32)),
        });
    }
    let optimistic = forward
        .iter()
        .map(|p| ScenarioRow {
            period: p.period,
            revenue: py_round2(p.revenue * 1.1),
            expenses: py_round2(p.expenses * 0.95),
            profit: py_round2(p.revenue * 1.1 - p.expenses * 0.95),
        })
        .collect();
    let pessimistic = forward
        .iter()
        .map(|p| ScenarioRow {
            period: p.period,
            revenue: py_round2(p.revenue * 0.9),
            expenses: py_round2(p.expenses * 1.05),
            profit: py_round2(p.revenue * 0.9 - p.expenses * 1.05),
        })
        .collect();
    RollingForecastResult {
        company_id: company_id.to_string(),
        base_actual: 1_000_000.0,
        forward_projections: forward,
        optimistic,
        pessimistic,
        recommended_scenario: "base".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepare_and_monitor() {
        let p = prepare_budget("c1", 1_000_000.0, 600_000.0, 250_000.0);
        assert_eq!(p.gross_profit, 400_000.0);
        assert_eq!(p.ebitda, 150_000.0);
        let m = monitor_budget("c1", 1000.0, 1040.0);
        assert_eq!(m.variance, 40.0);
        assert_eq!(m.variance_pct, 4.0);
        assert_eq!(m.status, "on_track");
        let m2 = monitor_budget("c1", 1000.0, 1060.0);
        assert_eq!(m2.status, "attention_needed");
        assert_eq!(monitor_budget("c1", 0.0, 50.0).variance_pct, 0.0);
    }

    #[test]
    fn variance_analysis_flags_and_recommends() {
        let req = BudgetVarianceRequest {
            company_id: "c1".into(),
            period: "2026-09".into(),
            cost_center: "ops".into(),
            items: vec![
                BudgetItem {
                    item_id: "1".into(),
                    item_name: "Rent".into(),
                    category: "fixed".into(),
                    budget: 1000.0,
                    actual: 1150.0,
                },
                BudgetItem {
                    item_id: "2".into(),
                    item_name: "Power".into(),
                    category: "variable".into(),
                    budget: 1000.0,
                    actual: 950.0,
                },
            ],
        };
        let r = analyze_budget_variance(&req);
        assert_eq!(r.summary.total_budget, 2000.0);
        assert_eq!(r.summary.total_actual, 2100.0);
        assert_eq!(r.summary.total_variance, 100.0);
        assert_eq!(r.summary.variance_pct, 5.0);
        // 15% over -> flagged; 5% under -> not
        assert_eq!(r.significant_variances.len(), 1);
        assert_eq!(r.significant_variances[0].item, "Rent");
        assert!(!r.item_analysis[0].favorable);
        assert!(r.item_analysis[1].favorable);
        // exactly 5% is not > 5 -> no recommendation
        assert!(r.recommendations.is_empty());
    }

    #[test]
    fn detailed_variance_split() {
        let res = vec![
            ActualResult {
                account_id: Some("a1".into()),
                description: "sales".into(),
                budget: 1000.0,
                actual: 900.0,
            },
            ActualResult {
                account_id: Some("a2".into()),
                description: "costs".into(),
                budget: 500.0,
                actual: 560.0,
            },
        ];
        let r = detailed_variance_analysis("c1", "b1", &res, 5.0);
        assert_eq!(r.favorable_variance, 100.0);
        assert_eq!(r.unfavorable_variance, 60.0);
        assert_eq!(r.total_variance, -40.0);
        assert_eq!(r.items_outside_tolerance.len(), 2);
        assert_eq!(r.corrective_actions.len(), 3);
    }

    #[test]
    fn forecast_with_seasonality() {
        let mut seasons = BTreeMap::new();
        seasons.insert("Q2".to_string(), 1.2);
        let req = ForecastRequest {
            company_id: "c1".into(),
            periods: 4,
            forecast_method: "growth_rate".into(),
            historical_data: vec![100.0, 200.0],
            growth_rates: BTreeMap::new(),
            seasonal_factors: seasons,
        };
        let f = generate_forecast(&req);
        assert_eq!(f.projections.len(), 4);
        // period 1: 300 * 1.03^1 * 1.0 = 309
        assert_eq!(f.projections[0].projected_value, 309.0);
        // period 4: Q2 -> 300 * 1.03^4 * 1.2
        assert_eq!(
            f.projections[3].projected_value,
            py_round2(300.0 * 1.03f64.powi(4) * 1.2)
        );
        assert_eq!(f.projections[3].seasonal_factor, 1.2);
        assert_eq!(f.confidence_intervals[0].upper, py_round2(309.0 * 1.1));
        assert_eq!(f.accuracy_score, 0.92);
    }

    #[test]
    fn rolling_forecast_scenarios() {
        let r = rolling_forecast("c1", 2);
        assert_eq!(r.forward_projections.len(), 2);
        assert_eq!(r.optimistic[0].revenue, 1_155_000.0);
        assert_eq!(r.pessimistic[0].revenue, 945_000.0);
        assert_eq!(r.recommended_scenario, "base");
    }
}
