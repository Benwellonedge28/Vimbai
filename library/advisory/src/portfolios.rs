//! Portfolio and rate-risk advisory: fund look-through, investment portfolio
//! returns and repricing gap analysis. Ported from fund-management-service,
//! investment-portfolio-service and interest-rate-risk-service.
//!
//! Kept as shipped:
//!
//! * Fund weights are rounded per holding and the top-10 figure is the sum of
//!   those rounded weights, so a three-holding fund can report 99.99%.
//!   Fund return is a fixed 8% and tracking error a fixed 2%.
//! * Portfolio volatility treats the weighted volatilities as independent
//!   (root of the sum of squares) and Sharpe uses the arithmetic portfolio
//!   return.
//! * Rate risk walks six fixed buckets, so any other bucket label is ignored;
//!   duration gap is hardcoded at 1.5 and earnings-at-risk at ten percent of
//!   liabilities times the shock.

use crate::{div, num, r2, rows, text};
use serde_json::{json, Value};

pub fn analyze_fund_management(request: &Value) -> Value {
    let holdings = rows(request, "holdings");
    let total_nav: f64 = holdings
        .iter()
        .map(|h| num(h, "shares") * num(h, "price"))
        .sum();

    let mut analysed: Vec<Value> = holdings
        .iter()
        .map(|h| {
            let value = num(h, "shares") * num(h, "price");
            json!({
                "holding_id": text(h, "holding_id"),
                "security_name": text(h, "security_name"),
                "value": r2(value),
                "weight": r2(div(value, total_nav) * 100.0),
                "shares": num(h, "shares"),
                "price": num(h, "price"),
            })
        })
        .collect();
    // Stable descending sort on value, as Python's sort(reverse=True).
    analysed.sort_by(|a, b| {
        b["value"]
            .as_f64()
            .unwrap_or(0.0)
            .partial_cmp(&a["value"].as_f64().unwrap_or(0.0))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let top_holdings: Vec<Value> = analysed.iter().take(10).cloned().collect();
    let top10_weight: f64 = top_holdings
        .iter()
        .map(|h| h["weight"].as_f64().unwrap_or(0.0))
        .sum();

    let benchmark_return = num(request, "benchmark_return");
    let alpha = 0.05 - benchmark_return;
    let tracking_error = 0.02;
    let info_ratio = div(alpha, tracking_error);

    let mut recommendations = Vec::new();
    if top10_weight > 50.0 {
        recommendations
            .push("High concentration in top 10 holdings - consider diversifying".to_string());
    }
    if alpha < 0.0 {
        recommendations.push("Negative alpha - review investment strategy".to_string());
    }
    if holdings.len() < 20 {
        recommendations.push("Low diversification - add more holdings".to_string());
    }

    json!({
        "fund_id": text(request, "fund_id"),
        "fund_name": text(request, "fund_name"),
        "fund_metrics": {
            "total_nav": r2(total_nav),
            "holding_count": holdings.len(),
            "top10_weight": r2(top10_weight),
        },
        "top_holdings": top_holdings,
        "performance_metrics": {
            "fund_return": 0.08,
            "benchmark_return": benchmark_return,
            "alpha": r2(alpha * 100.0),
            "tracking_error": r2(tracking_error * 100.0),
            "information_ratio": vimbai_common::py_round(info_ratio, 4),
        },
        "recommendations": recommendations,
    })
}

pub fn analyze_investment_portfolio(request: &Value) -> Value {
    let investments = rows(request, "investments");
    let total_value: f64 = investments.iter().map(|i| num(i, "current_value")).sum();
    let total_cost: f64 = investments.iter().map(|i| num(i, "cost_basis")).sum();
    let total_gain = total_value - total_cost;

    // Python dict preserves first-seen order, so keep it explicitly.
    let mut by_type: Vec<(String, f64, f64)> = Vec::new();
    for inv in investments {
        let kind = text(inv, "asset_type");
        let value = num(inv, "current_value");
        let cost = num(inv, "cost_basis");
        match by_type.iter_mut().find(|(t, _, _)| *t == kind) {
            Some(entry) => {
                entry.1 += value;
                entry.2 += cost;
            }
            None => by_type.push((kind, value, cost)),
        }
    }

    let allocation_analysis: Vec<Value> = by_type
        .iter()
        .map(|(kind, value, cost)| {
            json!({
                "asset_type": kind,
                "value": r2(*value),
                "cost": r2(*cost),
                "allocation_pct": r2(div(*value, total_value) * 100.0),
                "gain": r2(value - cost),
            })
        })
        .collect();

    let port_return = if total_value == 0.0 {
        0.0
    } else {
        investments
            .iter()
            .map(|i| num(i, "current_value") * num(i, "expected_return"))
            .sum::<f64>()
            / total_value
    };
    let port_vol = if total_value == 0.0 {
        0.0
    } else {
        investments
            .iter()
            .map(|i| (num(i, "current_value") / total_value).powi(2) * num(i, "volatility").powi(2))
            .sum::<f64>()
            .sqrt()
    };
    let sharpe = div(port_return - num(request, "risk_free_rate"), port_vol);

    let mut recommendations = Vec::new();
    if sharpe < 0.5 {
        recommendations.push("Low Sharpe ratio - consider rebalancing portfolio".to_string());
    }
    if port_vol > 0.2 {
        recommendations.push("High volatility - consider reducing risk".to_string());
    }
    if (total_value - total_cost) / total_cost * 100.0 < 0.0 {
        recommendations.push("Portfolio in loss - review asset allocation".to_string());
    }

    json!({
        "company_id": text(request, "company_id"),
        "portfolio_summary": {
            "total_value": r2(total_value),
            "total_cost": r2(total_cost),
            "total_gain": r2(total_gain),
            "total_return_pct": r2(if total_cost == 0.0 { 0.0 } else { total_gain / total_cost * 100.0 }),
            "asset_count": investments.len(),
        },
        "allocation_analysis": allocation_analysis,
        "risk_metrics": {
            "portfolio_return": r2(port_return * 100.0),
            "portfolio_volatility": r2(port_vol * 100.0),
            "sharpe_ratio": vimbai_common::py_round(sharpe, 4),
            "risk_free_rate": r2(num(request, "risk_free_rate") * 100.0),
        },
        "recommendations": recommendations,
    })
}

/// Python never prints a negative zero; keep the wire clean.
fn pos_zero(value: f64) -> f64 {
    if value == 0.0 {
        0.0
    } else {
        value
    }
}

pub fn analyze_interest_rate_risk(request: &Value) -> Value {
    let assets = rows(request, "rate_sensitive_assets");
    let liabilities = rows(request, "rate_sensitive_liabilities");
    let total_assets: f64 = assets.iter().map(|a| num(a, "balance")).sum();
    let total_liabilities: f64 = liabilities.iter().map(|l| num(l, "balance")).sum();

    let buckets = ["0-3m", "3-6m", "6-12m", "1-2y", "2-5y", "5y+"];
    let mut gaps = Vec::new();
    let mut cumulative = 0.0;
    for bucket in buckets {
        let asset_gap: f64 = assets
            .iter()
            .filter(|a| text(a, "repricing_bucket") == bucket)
            .map(|a| num(a, "balance"))
            .sum();
        let liability_gap: f64 = liabilities
            .iter()
            .filter(|l| text(l, "repricing_bucket") == bucket)
            .map(|l| num(l, "balance"))
            .sum();
        let net_gap = pos_zero(asset_gap - liability_gap);
        cumulative += net_gap;
        gaps.push(json!({
            "bucket": bucket,
            "asset_gap": pos_zero(asset_gap),
            "liability_gap": pos_zero(liability_gap),
            "net_gap": net_gap,
            "cumulative_gap": cumulative,
        }));
    }

    let rate_shock = num(request, "current_rate_shock") / 100.0;
    let nii_impact = cumulative * rate_shock;
    let economic_value = total_assets * rate_shock * 0.5;
    let earnings_at_risk = total_liabilities * rate_shock * 0.1;
    let sensitivity = (nii_impact / total_assets).abs();
    let rating = if sensitivity < 0.05 {
        "LOW"
    } else if sensitivity < 0.10 {
        "MEDIUM"
    } else {
        "HIGH"
    };

    json!({
        "company_id": text(request, "company_id"),
        "total_sensitive_assets": total_assets,
        "total_sensitive_liabilities": total_liabilities,
        "gap_analysis": gaps,
        "rate_sensitivity_nii": nii_impact,
        "economic_value_change": economic_value,
        "duration_gap": 1.5,
        "earnings_at_risk": earnings_at_risk,
        "risk_rating": rating,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn normalise(value: &Value) -> Value {
        match value {
            Value::Number(n) => json!(n.as_f64().unwrap_or(0.0)),
            Value::Array(items) => Value::Array(items.iter().map(normalise).collect()),
            Value::Object(map) => {
                Value::Object(map.iter().map(|(k, v)| (k.clone(), normalise(v))).collect())
            }
            other => other.clone(),
        }
    }

    fn assert_parity(actual: Value, expected: &str) {
        let expected: Value = serde_json::from_str(expected).expect("expected json");
        assert_eq!(
            serde_json::to_string(&normalise(&actual)).unwrap(),
            serde_json::to_string(&normalise(&expected)).unwrap()
        );
    }

    #[test]
    fn fund_management_matches_the_service() {
        let request = json!({"fund_id": "F1", "fund_name": "Growth", "benchmark_return": 0.06,
        "risk_free_rate": 0.02, "holdings": [
            {"holding_id": "H1", "security_name": "Alpha", "shares": 100, "price": 50, "weight": 0},
            {"holding_id": "H2", "security_name": "Beta", "shares": 200, "price": 25, "weight": 0},
            {"holding_id": "H3", "security_name": "Gamma", "shares": 10, "price": 100, "weight": 0}
        ]});
        let expected = r#"{
            "fund_id": "F1",
            "fund_name": "Growth",
            "fund_metrics": {"total_nav": 11000.0, "holding_count": 3, "top10_weight": 99.99},
            "top_holdings": [
                {"holding_id": "H1", "security_name": "Alpha", "value": 5000.0, "weight": 45.45,
                 "shares": 100.0, "price": 50.0},
                {"holding_id": "H2", "security_name": "Beta", "value": 5000.0, "weight": 45.45,
                 "shares": 200.0, "price": 25.0},
                {"holding_id": "H3", "security_name": "Gamma", "value": 1000.0, "weight": 9.09,
                 "shares": 10.0, "price": 100.0}
            ],
            "performance_metrics": {"fund_return": 0.08, "benchmark_return": 0.06, "alpha": -1.0,
                                    "tracking_error": 2.0, "information_ratio": -0.5},
            "recommendations": ["High concentration in top 10 holdings - consider diversifying",
                                "Negative alpha - review investment strategy",
                                "Low diversification - add more holdings"]
        }"#;
        assert_parity(analyze_fund_management(&request), expected);
    }

    #[test]
    fn investment_portfolio_matches_the_service() {
        let request = json!({"company_id": "C1", "risk_free_rate": 0.03, "target_return": 0.08,
        "investments": [
            {"asset_id": "X1", "asset_type": "equity", "current_value": 600, "cost_basis": 500,
             "expected_return": 0.09, "volatility": 0.18},
            {"asset_id": "X2", "asset_type": "bond", "current_value": 400, "cost_basis": 450,
             "expected_return": 0.04, "volatility": 0.06}
        ]});
        let expected = r#"{
            "company_id": "C1",
            "portfolio_summary": {"total_value": 1000.0, "total_cost": 950.0, "total_gain": 50.0,
                                  "total_return_pct": 5.26, "asset_count": 2},
            "allocation_analysis": [
                {"asset_type": "equity", "value": 600.0, "cost": 500.0, "allocation_pct": 60.0,
                 "gain": 100.0},
                {"asset_type": "bond", "value": 400.0, "cost": 450.0, "allocation_pct": 40.0,
                 "gain": -50.0}
            ],
            "risk_metrics": {"portfolio_return": 7.0, "portfolio_volatility": 11.06,
                             "sharpe_ratio": 0.3616, "risk_free_rate": 3.0},
            "recommendations": ["Low Sharpe ratio - consider rebalancing portfolio"]
        }"#;
        assert_parity(analyze_investment_portfolio(&request), expected);
    }

    #[test]
    fn interest_rate_risk_matches_the_service() {
        let request = json!({"company_id": "C1", "reporting_date": "2026-10-31",
        "current_rate_shock": 2.0,
        "rate_sensitive_assets": [
            {"item_id": "A1", "description": "Loan book", "balance": 1000, "rate_type": "variable",
             "maturity_date": "2027-01-01", "repricing_bucket": "0-3m"},
            {"item_id": "A2", "description": "Bonds", "balance": 500, "rate_type": "fixed",
             "maturity_date": "2028-01-01", "repricing_bucket": "1-2y"}
        ],
        "rate_sensitive_liabilities": [
            {"item_id": "L1", "description": "Deposits", "balance": 800, "rate_type": "variable",
             "maturity_date": "2027-01-01", "repricing_bucket": "0-3m"},
            {"item_id": "L2", "description": "Long bond", "balance": 300, "rate_type": "fixed",
             "maturity_date": "2032-01-01", "repricing_bucket": "5y+"}
        ]});
        let expected = r#"{
            "company_id": "C1",
            "total_sensitive_assets": 1500.0,
            "total_sensitive_liabilities": 1100.0,
            "gap_analysis": [
                {"bucket": "0-3m", "asset_gap": 1000.0, "liability_gap": 800.0, "net_gap": 200.0,
                 "cumulative_gap": 200.0},
                {"bucket": "3-6m", "asset_gap": 0.0, "liability_gap": 0.0, "net_gap": 0.0,
                 "cumulative_gap": 200.0},
                {"bucket": "6-12m", "asset_gap": 0.0, "liability_gap": 0.0, "net_gap": 0.0,
                 "cumulative_gap": 200.0},
                {"bucket": "1-2y", "asset_gap": 500.0, "liability_gap": 0.0, "net_gap": 500.0,
                 "cumulative_gap": 700.0},
                {"bucket": "2-5y", "asset_gap": 0.0, "liability_gap": 0.0, "net_gap": 0.0,
                 "cumulative_gap": 700.0},
                {"bucket": "5y+", "asset_gap": 0.0, "liability_gap": 300.0, "net_gap": -300.0,
                 "cumulative_gap": 400.0}
            ],
            "rate_sensitivity_nii": 8.0,
            "economic_value_change": 15.0,
            "duration_gap": 1.5,
            "earnings_at_risk": 2.2,
            "risk_rating": "LOW"
        }"#;
        assert_parity(analyze_interest_rate_risk(&request), expected);
    }

    #[test]
    fn an_off_menu_bucket_is_dropped_silently() {
        let request = json!({"company_id": "C1", "current_rate_shock": 2.0,
            "rate_sensitive_assets": [
                {"balance": 100, "repricing_bucket": "7-9m"},
                {"balance": 50, "repricing_bucket": "2-5y"}
            ],
            "rate_sensitive_liabilities": []});
        let out = analyze_interest_rate_risk(&request);
        // total assets still counts both, but only the 2-5y bucket carries a gap
        assert_eq!(out["total_sensitive_assets"], json!(150.0));
        assert_eq!(out["gap_analysis"][4]["asset_gap"], json!(50.0));
        assert_eq!(out["gap_analysis"][5]["asset_gap"], json!(0.0));
    }
}
