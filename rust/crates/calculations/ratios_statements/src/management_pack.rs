//! The management reporting pack: the dashboard sweep and the planning projections.
//!
//! These two came from the Python services of the same name and keep their arithmetic exactly,
//! including the decisions the Python made that a reader might not expect:
//!
//! * a metric exactly on its target counts as on target, and the threshold is 90%, not 100%,
//!   for "on target" (>= 100 is above target),
//! * the health score counts on-target and above-target together,
//! * a metric with no target divides by nothing and reports 0% rather than failing,
//! * planning growth is compounded from period 1, not period 0.
//!
//! The dashboard date is taken from the request when the caller supplies one. Python stamped
//! `datetime.now()`, which cannot be reproduced from a pure function; the caller owns the clock.

use serde_json::{Map, Value};

/// Round the way the Python did: two decimal places.
fn round2(value: f64) -> f64 {
    vimbai_common::numbers::round_dp(value, 2)
}

fn number(value: Option<&Value>) -> f64 {
    match value {
        Some(Value::Number(n)) => n.as_f64().unwrap_or(0.0),
        Some(Value::String(s)) => s.trim().parse().unwrap_or(0.0),
        _ => 0.0,
    }
}

/// `POST /generate` of the management dashboard service.
pub fn generate_dashboard(payload: &Value) -> Result<Value, String> {
    let metrics = payload
        .get("metrics")
        .and_then(Value::as_array)
        .ok_or_else(|| "metrics is required".to_string())?;

    let mut analysis: Vec<(f64, Value)> = Vec::with_capacity(metrics.len());
    let mut on_target = 0u32;
    let mut above_target = 0u32;
    let mut below_target = 0u32;
    let mut alerts: Vec<Value> = Vec::new();

    for metric in metrics {
        let current = number(metric.get("current_value"));
        let target = number(metric.get("target_value"));
        let pct = if target == 0.0 {
            0.0
        } else {
            current / target * 100.0
        };
        let variance = current - target;

        let status = if pct >= 100.0 {
            above_target += 1;
            "Above Target"
        } else if pct >= 90.0 {
            on_target += 1;
            "On Target"
        } else {
            below_target += 1;
            // the Python formatted this as abs(100 - pct) to one decimal place
            let shortfall = format!("{:.1}", (100.0 - pct).abs());
            alerts.push(Value::String(format!(
                "{} is {}% below target",
                metric
                    .get("metric_name")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
                shortfall
            )));
            "Below Target"
        };

        let trend = metric
            .get("trend")
            .and_then(Value::as_str)
            .unwrap_or_default();
        analysis.push((
            round2(pct),
            serde_json::json!({
                "metric": metric.get("metric_name").cloned().unwrap_or(Value::Null),
                "current": round2(current),
                "target": round2(target),
                "variance": round2(variance),
                "pct_of_target": round2(pct),
                "trend": trend,
                "status": status,
            }),
        ));
    }

    // sorted by achievement, best first; Python's sort is stable, and so is this one
    analysis.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    let ranked: Vec<Value> = analysis.into_iter().map(|(_, row)| row).collect();
    let top: Vec<Value> = ranked
        .iter()
        .take(3)
        .filter_map(|row| row.get("metric").cloned())
        .collect();

    let total = metrics.len() as f64;
    let health_score = if total == 0.0 {
        0.0
    } else {
        (on_target + above_target) as f64 / total * 100.0
    };

    let mut out = Map::new();
    out.insert(
        "company_id".into(),
        payload.get("company_id").cloned().unwrap_or(Value::Null),
    );
    out.insert(
        "dashboard_date".into(),
        payload
            .get("dashboard_date")
            .cloned()
            .unwrap_or(Value::String(String::new())),
    );
    out.insert(
        "kpi_summary".into(),
        serde_json::json!({
            "total_metrics": metrics.len(),
            "on_target": on_target,
            "above_target": above_target,
            "below_target": below_target,
            "health_score": round2(health_score),
        }),
    );
    out.insert("metric_analysis".into(), Value::Array(ranked));
    out.insert("alerts".into(), Value::Array(alerts));
    out.insert("top_performers".into(), Value::Array(top));
    Ok(Value::Object(out))
}

/// `POST /plan` of the financial planning service.
///
/// The defaults are the Python's: ten million of revenue and ten percent growth when the caller
/// supplies neither, and costs at 70%, capital at 10%, working capital at 15% of revenue.
pub fn create_financial_plan(payload: &Value) -> Result<Value, String> {
    let historical = payload
        .get("historical_data")
        .cloned()
        .unwrap_or(Value::Null);
    let growth = payload
        .get("growth_assumptions")
        .cloned()
        .unwrap_or(Value::Null);

    let base_revenue = historical
        .get("revenue")
        .map(|v| number(Some(v)))
        .filter(|v| *v != 0.0)
        .unwrap_or(10_000_000.0);
    let growth_rate = match growth.get("revenue_growth") {
        Some(value) => number(Some(value)),
        None => 0.1,
    };

    let mut revenue = Vec::with_capacity(5);
    let mut costs = Vec::with_capacity(5);
    let mut capex = Vec::with_capacity(5);
    let mut working_capital = Vec::with_capacity(5);
    for period in 1..=5 {
        let projected = base_revenue * (1.0 + growth_rate).powi(period);
        revenue.push(round2(projected));
        costs.push(round2(projected * 0.7));
        capex.push(round2(projected * 0.1));
        working_capital.push(round2(projected * 0.15));
    }

    // the sensitivity lines are computed from the unrounded projections, as in Python
    let mut growth_down = Vec::with_capacity(5);
    let mut cost_up = Vec::with_capacity(5);
    for period in 1..=5 {
        let projected = base_revenue * (1.0 + growth_rate).powi(period);
        growth_down.push(projected * 0.9);
        cost_up.push(projected * 0.7 * 1.05);
    }

    let mut out = Map::new();
    out.insert(
        "company_id".into(),
        payload.get("company_id").cloned().unwrap_or(Value::Null),
    );
    out.insert(
        "fiscal_year".into(),
        payload.get("fiscal_year").cloned().unwrap_or(Value::Null),
    );
    out.insert(
        "financial_plan".into(),
        serde_json::json!({
            "company_id": payload.get("company_id").cloned().unwrap_or(Value::Null),
            "planning_period": payload.get("fiscal_year").cloned().unwrap_or(Value::Null),
            "revenue_projections": revenue,
            "cost_projections": costs,
            "capital_expenditure_plan": capex,
            "working_capital_requirements": working_capital,
            "financing_plan": {"debt_ratio": 0.4, "equity_ratio": 0.6},
        }),
    );
    out.insert("key_assumptions".into(), growth);
    out.insert(
        "sensitivity_analysis".into(),
        serde_json::json!({"revenue_growth": growth_down, "cost_increase": cost_up}),
    );
    out.insert(
        "recommendations".into(),
        serde_json::json!([
            "Monitor revenue growth assumptions quarterly",
            "Review cost structure annually",
            "Maintain capital reserve for contingencies",
        ]),
    );
    Ok(Value::Object(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_target_of_zero_reports_zero_percent_instead_of_dividing_by_nothing() {
        let out = generate_dashboard(&serde_json::json!({
            "company_id": "c-1",
            "metrics": [{"metric_name": "Headcount", "current_value": 12.0, "target_value": 0.0, "trend": "flat"}]
        }))
        .unwrap();
        assert_eq!(
            out["metric_analysis"][0]["pct_of_target"],
            serde_json::json!(0.0)
        );
        assert_eq!(out["kpi_summary"]["below_target"], serde_json::json!(1));
    }

    #[test]
    fn ninety_percent_counts_as_on_target() {
        let out = generate_dashboard(&serde_json::json!({
            "company_id": "c-1",
            "metrics": [
                {"metric_name": "A", "current_value": 90.0, "target_value": 100.0, "trend": "flat"},
                {"metric_name": "B", "current_value": 100.0, "target_value": 100.0, "trend": "flat"}
            ]
        }))
        .unwrap();
        assert_eq!(out["kpi_summary"]["on_target"], serde_json::json!(1));
        assert_eq!(out["kpi_summary"]["above_target"], serde_json::json!(1));
        assert_eq!(out["kpi_summary"]["health_score"], serde_json::json!(100.0));
    }

    #[test]
    fn planning_compounds_from_the_first_period() {
        let out = create_financial_plan(&serde_json::json!({
            "company_id": "c-1",
            "fiscal_year": "2026",
            "historical_data": {"revenue": 100.0},
            "growth_assumptions": {"revenue_growth": 0.5}
        }))
        .unwrap();
        assert_eq!(
            out["financial_plan"]["revenue_projections"][0],
            serde_json::json!(150.0)
        );
        assert_eq!(
            out["financial_plan"]["revenue_projections"][4],
            serde_json::json!(759.38)
        );
    }

    #[test]
    fn a_plan_without_assumptions_uses_the_python_defaults() {
        let out =
            create_financial_plan(&serde_json::json!({"company_id": "c-1", "fiscal_year": "2026"}))
                .unwrap();
        assert_eq!(
            out["financial_plan"]["revenue_projections"][0],
            serde_json::json!(11000000.0)
        );
    }
}
