//! Ratio analysis: financial ratios, peer percentiles and trend readings.
//!
//! Several defaults carry meaning and are kept: `market_cap` falls back to three times book equity
//! when the balance sheet has none, `ebit` falls back to net income, and interest expense falls back
//! to 1 so coverage never divides by zero.
//!
//! The `overall_score` is a blunt composite - current ratio halved, net margin plus fifty, hundred
//! minus twenty times debt-to-equity, asset turnover times fifty, each capped at 100, then averaged.
//! It is a presentation number, not a rating, and a ratio of 2.0 contributes exactly as much as 5.0.
//!
//! One small divergence: when total assets equal current liabilities, ROCE divides by zero. Python
//! raised and returned a 500; this returns 0.0 for that line rather than failing the request.

use serde_json::{Map, Value};

fn round2(value: f64) -> f64 {
    vimbai_common::numbers::round_dp(value, 2)
}

fn round4(value: f64) -> f64 {
    vimbai_common::numbers::round_dp(value, 4)
}

fn get(map: Option<&Value>, key: &str, fallback: f64) -> f64 {
    map.and_then(|m| m.get(key))
        .and_then(Value::as_f64)
        .unwrap_or(fallback)
}

fn divide(top: f64, bottom: f64, when_zero: f64) -> f64 {
    if bottom == 0.0 {
        when_zero
    } else {
        top / bottom
    }
}

fn entry(map: &mut Map<String, Value>, key: &str, value: f64) {
    map.insert(key.to_string(), Value::from(value));
}

/// `POST /ratios`.
pub fn ratios(payload: &Value) -> Result<Value, String> {
    let bs = payload.get("balance_sheet");
    let is = payload.get("income_statement");

    let current_assets = get(bs, "current_assets", 0.0);
    let current_liabilities = get(bs, "current_liabilities", 0.0);
    let total_assets = get(bs, "total_assets", 1.0);
    let total_debt = get(bs, "total_debt", 0.0);
    let equity = get(bs, "total_equity", 0.0);
    let inventory = get(bs, "inventory", 0.0);
    let receivables = get(bs, "accounts_receivable", 0.0);
    let payables = get(bs, "accounts_payable", 0.0);
    let cash_and_securities = get(bs, "cash", 0.0) + get(bs, "marketable_securities", 0.0);
    let market_cap = get(bs, "market_cap", equity * 3.0);

    let revenue = get(is, "revenue", 0.0);
    let net_income = get(is, "net_income", 0.0);
    let ebit = get(is, "ebit", net_income);
    let cogs = get(is, "cogs", 0.0);
    let interest = {
        let declared = get(is, "interest_expense", 1.0);
        if declared == 0.0 {
            1.0
        } else {
            declared
        }
    };

    let mut liquidity = Map::new();
    entry(
        &mut liquidity,
        "current_ratio",
        round4(divide(current_assets, current_liabilities, 0.0)),
    );
    entry(
        &mut liquidity,
        "quick_ratio",
        round4(divide(current_assets - inventory, current_liabilities, 0.0)),
    );
    entry(
        &mut liquidity,
        "cash_ratio",
        round4(divide(cash_and_securities, current_liabilities, 0.0)),
    );
    entry(
        &mut liquidity,
        "working_capital",
        round2(current_assets - current_liabilities),
    );

    let mut profitability = Map::new();
    entry(
        &mut profitability,
        "gross_margin",
        round2(divide(revenue - cogs, revenue, 0.0) * 100.0),
    );
    entry(
        &mut profitability,
        "operating_margin",
        round2(divide(ebit, revenue, 0.0) * 100.0),
    );
    entry(
        &mut profitability,
        "net_margin",
        round2(divide(net_income, revenue, 0.0) * 100.0),
    );
    entry(
        &mut profitability,
        "roe",
        round2(divide(net_income, equity, 0.0) * 100.0),
    );
    entry(
        &mut profitability,
        "roa",
        round2(divide(net_income, total_assets, 0.0) * 100.0),
    );
    entry(
        &mut profitability,
        "roce",
        round2(divide(ebit, total_assets - current_liabilities, 0.0) * 100.0),
    );

    let mut leverage = Map::new();
    entry(
        &mut leverage,
        "debt_to_equity",
        round4(divide(total_debt, equity, 0.0)),
    );
    entry(
        &mut leverage,
        "debt_to_assets",
        round4(divide(total_debt, total_assets, 0.0)),
    );
    entry(&mut leverage, "interest_coverage", round2(ebit / interest));
    entry(
        &mut leverage,
        "equity_multiplier",
        round4(divide(total_assets, equity, 0.0)),
    );

    let mut efficiency = Map::new();
    entry(
        &mut efficiency,
        "asset_turnover",
        round4(divide(revenue, total_assets, 0.0)),
    );
    entry(
        &mut efficiency,
        "inventory_turnover",
        round4(divide(cogs, inventory, 0.0)),
    );
    entry(
        &mut efficiency,
        "ar_turnover",
        round4(divide(revenue, receivables, 0.0)),
    );
    entry(
        &mut efficiency,
        "ap_turnover",
        round4(divide(cogs, payables, 0.0)),
    );
    entry(
        &mut efficiency,
        "days_sales_outstanding",
        round2(divide(receivables, revenue, 0.0) * 365.0),
    );

    let mut valuation = Map::new();
    entry(
        &mut valuation,
        "pe_ratio",
        round2(divide(market_cap, net_income, 0.0)),
    );
    entry(
        &mut valuation,
        "pb_ratio",
        round2(divide(market_cap, equity, 0.0)),
    );
    entry(
        &mut valuation,
        "ps_ratio",
        round2(divide(market_cap, revenue, 0.0)),
    );

    let number = |map: &Map<String, Value>, key: &str| -> f64 {
        map.get(key).and_then(Value::as_f64).unwrap_or(0.0)
    };
    let score = (number(&liquidity, "current_ratio") / 2.0).min(100.0)
        + (number(&profitability, "net_margin") + 50.0).min(100.0)
        + (100.0 - number(&leverage, "debt_to_equity") * 20.0).min(100.0)
        + (number(&efficiency, "asset_turnover") * 50.0).min(100.0);
    let overall = score / 4.0;

    let mut recommendations = Vec::new();
    if number(&liquidity, "current_ratio") < 1.5 {
        recommendations.push(Value::String("Improve liquidity position".into()));
    }
    if number(&profitability, "net_margin") < 10.0 {
        recommendations.push(Value::String("Focus on cost reduction".into()));
    }
    if number(&leverage, "debt_to_equity") > 2.0 {
        recommendations.push(Value::String("Consider debt reduction".into()));
    }

    let mut out = Map::new();
    out.insert(
        "company_id".into(),
        payload.get("company_id").cloned().unwrap_or(Value::Null),
    );
    out.insert(
        "period".into(),
        payload.get("period").cloned().unwrap_or(Value::Null),
    );
    out.insert("liquidity_ratios".into(), Value::Object(liquidity));
    out.insert("profitability_ratios".into(), Value::Object(profitability));
    out.insert("leverage_ratios".into(), Value::Object(leverage));
    out.insert("efficiency_ratios".into(), Value::Object(efficiency));
    out.insert("valuation_ratios".into(), Value::Object(valuation));
    out.insert("overall_score".into(), Value::from(round2(overall)));
    out.insert("recommendations".into(), Value::Array(recommendations));
    Ok(Value::Object(out))
}

/// `POST /peer-comparison`.
///
/// The percentile is the position of the company's value among peers plus itself, so a company that
/// beats every peer lands at `n / (n+1)` of the range and never reports 100. Ties take the first
/// matching position, which flatters a company that merely matches its best peer.
pub fn peer_comparison(payload: &Value) -> Result<Value, String> {
    let company = payload
        .get("company_ratios")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let peers = payload
        .get("peer_ratios")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let mut rankings = Map::new();
    let mut strengths = Vec::new();
    let mut weaknesses = Vec::new();
    let mut improvement = Vec::new();

    for (ratio, value) in &company {
        let value = value.as_f64().unwrap_or(0.0);
        let mut values: Vec<f64> = peers
            .iter()
            .map(|peer| peer.get(ratio).and_then(Value::as_f64).unwrap_or(0.0))
            .collect();
        values.push(value);
        values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let position = values.iter().position(|v| *v == value).unwrap_or(0);
        let percentile = position as f64 / values.len() as f64 * 100.0;
        rankings.insert(ratio.clone(), Value::from(round2(percentile)));
        if percentile >= 70.0 {
            strengths.push(Value::String(ratio.clone()));
        }
        if percentile < 30.0 {
            weaknesses.push(Value::String(ratio.clone()));
            improvement.push(serde_json::json!({"ratio": ratio, "industry_avg": 50}));
        }
    }

    let mut out = Map::new();
    out.insert(
        "company_id".into(),
        payload.get("company_id").cloned().unwrap_or(Value::Null),
    );
    out.insert(
        "industry".into(),
        payload.get("industry").cloned().unwrap_or(Value::Null),
    );
    out.insert("percentile_rankings".into(), Value::Object(rankings));
    out.insert("strengths".into(), Value::Array(strengths));
    out.insert("weaknesses".into(), Value::Array(weaknesses));
    out.insert("improvement_areas".into(), Value::Array(improvement));
    Ok(Value::Object(out))
}

/// `POST /trend`.
///
/// A series needs two points to earn a change, a momentum word and a prediction; a single point
/// reports its current value and "stable" and gets no forecast. The forecast is the last value plus
/// two percent, so a falling series is still predicted to rise. `alerts` is always empty.
pub fn trend(payload: &Value) -> Result<Value, String> {
    let history = payload
        .get("ratio_history")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    let mut summary = Map::new();
    let mut momentum = Map::new();
    let mut predictions = Map::new();

    for (ratio, series) in &history {
        let values: Vec<f64> = series
            .as_array()
            .map(|rows| rows.iter().filter_map(Value::as_f64).collect())
            .unwrap_or_default();
        if values.len() >= 2 {
            let first = values[0];
            let last = values[values.len() - 1];
            let change = last - first;
            let pct_change = if first == 0.0 {
                0.0
            } else {
                change / first * 100.0
            };
            let average = values.iter().sum::<f64>() / values.len() as f64;
            summary.insert(
                ratio.clone(),
                serde_json::json!({
                    "change": round4(change),
                    "pct_change": round2(pct_change),
                    "current": last,
                    "average": round4(average),
                }),
            );
            momentum.insert(
                ratio.clone(),
                Value::String(
                    if change > 0.0 {
                        "improving"
                    } else {
                        "declining"
                    }
                    .into(),
                ),
            );
            predictions.insert(ratio.clone(), Value::from(round4(last * 1.02)));
        } else {
            summary.insert(
                ratio.clone(),
                serde_json::json!({"current": values.first().copied().unwrap_or(0.0)}),
            );
            momentum.insert(ratio.clone(), Value::String("stable".into()));
        }
    }

    let mut out = Map::new();
    out.insert(
        "company_id".into(),
        payload.get("company_id").cloned().unwrap_or(Value::Null),
    );
    out.insert("trend_summary".into(), Value::Object(summary));
    out.insert("momentum".into(), Value::Object(momentum));
    out.insert("predictions".into(), Value::Object(predictions));
    out.insert("alerts".into(), serde_json::json!([]));
    Ok(Value::Object(out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn payload() -> Value {
        json!({
            "company_id": "c-1", "period": "FY2026",
            "balance_sheet": {
                "current_assets": 3_000_000.0, "current_liabilities": 2_000_000.0,
                "total_assets": 9_000_000.0, "total_debt": 5_000_000.0,
                "total_equity": 4_000_000.0, "inventory": 600_000.0,
                "accounts_receivable": 1_100_000.0, "accounts_payable": 450_000.0,
                "cash": 500_000.0, "marketable_securities": 250_000.0, "market_cap": 14_000_000.0
            },
            "income_statement": {
                "revenue": 10_000_000.0, "net_income": 800_000.0, "ebit": 1_200_000.0,
                "cogs": 6_000_000.0, "interest_expense": 300_000.0
            },
            "cash_flow": {"operating": 900_000.0}
        })
    }

    #[test]
    fn the_core_ratios_come_out_as_the_service_had_them() {
        let out = ratios(&payload()).unwrap();
        assert_eq!(out["liquidity_ratios"]["current_ratio"], json!(1.5));
        assert_eq!(out["liquidity_ratios"]["quick_ratio"], json!(1.2));
        assert_eq!(out["liquidity_ratios"]["cash_ratio"], json!(0.375));
        assert_eq!(out["profitability_ratios"]["roce"], json!(17.14));
        assert_eq!(
            out["efficiency_ratios"]["days_sales_outstanding"],
            json!(40.15)
        );
    }

    #[test]
    fn interest_coverage_survives_a_zero_interest_line() {
        let mut payload = payload();
        payload["income_statement"]["interest_expense"] = json!(0.0);
        let out = ratios(&payload).unwrap();
        assert_eq!(
            out["leverage_ratios"]["interest_coverage"],
            json!(1200000.0)
        );
    }

    #[test]
    fn market_cap_falls_back_to_three_times_book_equity() {
        let mut payload = payload();
        payload["balance_sheet"]
            .as_object_mut()
            .unwrap()
            .remove("market_cap");
        let out = ratios(&payload).unwrap();
        // 4.0m * 3 / 0.8m
        assert_eq!(out["valuation_ratios"]["pe_ratio"], json!(15.0));
    }

    #[test]
    fn the_composite_score_is_capped_per_component() {
        let out = ratios(&payload()).unwrap();
        // 0.75 + 58 + 75 + 55.56 = 189.31, over four
        assert_eq!(out["overall_score"], json!(47.33));
    }

    #[test]
    fn recommendations_follow_the_three_thresholds() {
        let out = ratios(&payload()).unwrap();
        // current ratio is exactly 1.5 and the test is strictly less than, so liquidity passes
        assert_eq!(out["recommendations"], json!(["Focus on cost reduction"]));
    }

    #[test]
    fn an_empty_balance_sheet_reports_zeroes_rather_than_failing() {
        let out = ratios(&json!({"company_id": "c-2", "period": "FY2026"})).unwrap();
        assert_eq!(out["liquidity_ratios"]["current_ratio"], json!(0.0));
        assert_eq!(out["leverage_ratios"]["debt_to_assets"], json!(0.0));
    }

    #[test]
    fn a_percentile_never_reaches_one_hundred() {
        let out = peer_comparison(&json!({
            "company_id": "c-1", "industry": "Retail",
            "company_ratios": {"current_ratio": 1.5},
            "peer_ratios": [{"current_ratio": 2.0}, {"current_ratio": 1.1}, {"current_ratio": 1.7}]
        }))
        .unwrap();
        // third of four positions: one peer below it
        assert_eq!(out["percentile_rankings"]["current_ratio"], json!(25.0));
        assert_eq!(out["weaknesses"], json!(["current_ratio"]));
    }

    #[test]
    fn ties_take_the_first_position() {
        let out = peer_comparison(&json!({
            "company_id": "c-1", "industry": "Retail",
            "company_ratios": {"net_margin": 12.0},
            "peer_ratios": [{"net_margin": 12.0}, {"net_margin": 5.0}]
        }))
        .unwrap();
        // tied with the best peer: still only one of three positions
        assert_eq!(out["percentile_rankings"]["net_margin"], json!(33.33));
    }

    #[test]
    fn a_single_point_series_gets_no_forecast() {
        let out = trend(&json!({
            "company_id": "c-1", "periods": ["FY2026"],
            "ratio_history": {"quick_ratio": [0.9]}
        }))
        .unwrap();
        assert_eq!(out["trend_summary"]["quick_ratio"], json!({"current": 0.9}));
        assert_eq!(out["momentum"]["quick_ratio"], json!("stable"));
        assert!(out["predictions"].get("quick_ratio").is_none());
    }

    #[test]
    fn a_falling_series_is_still_predicted_upward() {
        let out = trend(&json!({
            "company_id": "c-1", "periods": ["FY2024", "FY2025", "FY2026"],
            "ratio_history": {"net_margin": [12.0, 10.0, 8.0]}
        }))
        .unwrap();
        assert_eq!(out["momentum"]["net_margin"], json!("declining"));
        assert_eq!(out["predictions"]["net_margin"], json!(8.16));
    }
}
