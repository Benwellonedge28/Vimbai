//! Audit-side advisory: transfer pricing compliance and analytical procedures.
//! Ported from transfer-pricing-service and analytical-procedures-service.
//!
//! Kept as shipped:
//!
//! * Transfer pricing compares each value with its own arm's-length range and
//!   reports the adjustment as the range bound minus the value, so an
//!   over-priced transaction carries a negative adjustment while the total is
//!   summed in absolute terms.
//! * Analytical procedures report an operating margin that is really an
//!   expense ratio, judge significance on unrounded variances (except gross
//!   margin, which uses the rounded ratios), and treat a tiny gap as an
//!   unexpected variance.

use crate::{div, num, r2, rows, text};
use serde_json::{json, Value};

pub fn analyze_transfer_pricing(request: &Value, analysis_date: &str) -> Value {
    let mut compliant = 0i64;
    let mut non_compliant = 0i64;
    let mut total_adjustment = 0.0;
    let mut results = Vec::new();

    for tx in rows(request, "transactions") {
        let value = num(tx, "transaction_value");
        let low = num(tx, "arm_length_range_min");
        let high = num(tx, "arm_length_range_max");
        let is_compliant = low <= value && value <= high;
        let adjustment = if is_compliant {
            compliant += 1;
            0.0
        } else {
            non_compliant += 1;
            if value < low {
                low - value
            } else {
                high - value
            }
        };
        if !is_compliant {
            total_adjustment += adjustment.abs();
        }
        results.push(json!({
            "id": text(tx, "id"),
            "product_service": text(tx, "product_service"),
            "selling_entity": text(tx, "selling_entity"),
            "buying_entity": text(tx, "buying_entity"),
            "transaction_value": value,
            "arm_length_range": [low, high],
            "method": text(tx, "method"),
            "compliant": is_compliant,
            "adjustment_needed": r2(adjustment),
        }));
    }

    json!({
        "company_id": text(request, "company_id"),
        "analysis_date": analysis_date,
        "compliant_transactions": compliant,
        "non_compliant_transactions": non_compliant,
        "total_adjustment_needed": r2(total_adjustment),
        "transactions": results,
        "documentation_required": [
            "Master File (OECD)",
            "Local File for each jurisdiction",
            "Country-by-Country report (if revenue threshold met)",
            "Benchmarking study supporting selected method",
        ],
    })
}

/// Python's `str(float)`: whole numbers keep one decimal (25.0, not 25).
fn py_float(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e15 {
        format!("{value:.1}")
    } else {
        format!("{value}")
    }
}

fn calc_ratio(current: f64, base: f64) -> f64 {
    if base == 0.0 {
        0.0
    } else {
        vimbai_common::py_round(current / base, 4)
    }
}

fn variance(current: f64, prior: f64) -> f64 {
    if prior == 0.0 {
        0.0
    } else {
        r2((current - prior) / prior * 100.0)
    }
}

fn status(value: f64) -> &'static str {
    if value.abs() > 20.0 {
        "significant_variance"
    } else {
        "within_expectations"
    }
}

fn field(data: &Value, key: &str) -> f64 {
    num(data, key)
}

pub fn analyze_analytical_procedures(request: &Value) -> Value {
    let current = request
        .get("current_data")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let prior = request
        .get("prior_period_data")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let benchmarks = request
        .get("industry_benchmarks")
        .cloned()
        .unwrap_or_else(|| json!({}));

    let benchmark = |key: &str, fallback: f64| -> f64 {
        benchmarks
            .get(key)
            .and_then(Value::as_f64)
            .unwrap_or(fallback)
    };

    let revenue = field(&current, "revenue");
    let prior_revenue = field(&prior, "revenue");
    let equity = field(&current, "equity");
    let prior_equity = field(&prior, "equity");
    let assets = field(&current, "total_assets");
    let prior_assets = field(&prior, "total_assets");

    // Gross margin judges significance on the rounded ratios; the other three
    // judge on the raw quotients.
    let gm_current = calc_ratio(field(&current, "gross_profit"), revenue);
    let gm_prior = calc_ratio(field(&prior, "gross_profit"), prior_revenue);
    let gm_status = if revenue != 0.0 && prior_revenue != 0.0 {
        status(variance(
            field(&current, "gross_profit") / revenue,
            field(&prior, "gross_profit") / prior_revenue,
        ))
    } else {
        status(0.0)
    };

    let opex_flag = revenue != 0.0 && prior_revenue != 0.0;
    let opex_var = if opex_flag {
        variance(
            field(&current, "operating_expenses") / revenue,
            field(&prior, "operating_expenses") / prior_revenue,
        )
    } else {
        0.0
    };
    let roa_flag = assets != 0.0 && prior_assets != 0.0;
    let roa_var = if roa_flag {
        variance(
            field(&current, "net_income") / assets,
            field(&prior, "net_income") / prior_assets,
        )
    } else {
        0.0
    };
    let de_flag = equity != 0.0 && prior_equity != 0.0;
    let de_var = if de_flag {
        variance(
            field(&current, "total_liabilities") / equity,
            field(&prior, "total_liabilities") / prior_equity,
        )
    } else {
        0.0
    };

    let ratios = vec![
        json!({
            "ratio_name": "gross_margin",
            "current_period": gm_current,
            "prior_period": gm_prior,
            "industry_benchmark": benchmark("gross_margin", 0.35),
            "variance_pct": variance(gm_current, gm_prior),
            "status": gm_status,
        }),
        json!({
            "ratio_name": "operating_margin",
            "current_period": calc_ratio(field(&current, "operating_expenses"), revenue),
            "prior_period": calc_ratio(field(&prior, "operating_expenses"), prior_revenue),
            "industry_benchmark": benchmark("operating_margin", 0.15),
            "variance_pct": opex_var,
            "status": status(opex_var),
        }),
        json!({
            "ratio_name": "return_on_assets",
            "current_period": calc_ratio(field(&current, "net_income"), assets),
            "prior_period": calc_ratio(field(&prior, "net_income"), prior_assets),
            "industry_benchmark": benchmark("roa", 0.08),
            "variance_pct": roa_var,
            "status": status(roa_var),
        }),
        json!({
            "ratio_name": "debt_to_equity",
            "current_period": calc_ratio(field(&current, "total_liabilities"), equity),
            "prior_period": calc_ratio(field(&prior, "total_liabilities"), prior_equity),
            "industry_benchmark": benchmark("debt_equity", 1.5),
            "variance_pct": de_var,
            "status": status(de_var),
        }),
    ];

    let unexpected: Vec<&Value> = ratios
        .iter()
        .filter(|r| r["status"] == json!("significant_variance"))
        .collect();
    let findings: Vec<Value> = unexpected
        .iter()
        .map(|r| {
            Value::String(format!(
                "Unusual {} variance of {}%",
                r["ratio_name"].as_str().unwrap_or(""),
                py_float(r["variance_pct"].as_f64().unwrap_or(0.0))
            ))
        })
        .collect();
    let reliance = if unexpected.len() > 2 {
        "low"
    } else if !unexpected.is_empty() {
        "moderate"
    } else {
        "high"
    };

    json!({
        "audit_id": text(request, "audit_id"),
        "ratio_analysis": ratios,
        "trend_indicators": {
            "revenue_growth": if revenue > prior_revenue { "increasing" } else { "decreasing" },
            "profitability": if field(&current, "net_income") > field(&prior, "net_income") { "improving" } else { "declining" },
            "leverage": if div(field(&current, "total_liabilities"), equity) > div(field(&prior, "total_liabilities"), prior_equity) { "increasing" } else { "decreasing" },
        },
        "unexpected_variances": unexpected
            .iter()
            .map(|r| json!({"ratio": r["ratio_name"], "variance": r["variance_pct"]}))
            .collect::<Vec<Value>>(),
        "analytical_findings": if findings.is_empty() {
            json!(["No significant unexpected variances identified"])
        } else {
            Value::Array(findings)
        },
        "conclusion": format!("Analytical procedures completed with {reliance} reliance on substantive testing"),
        "reliance_level": reliance,
    })
}

/// Internal control evaluation.
///
/// Ported from internal-control-evaluation-service. A control counts as
/// ineffective as soon as any deficiency is recorded against it, and the
/// engagement is effective only when neither a material weakness nor a
/// significant deficiency was found. The remediation list is fixed: the
/// service sends the same three suggestions whenever a material weakness
/// exists and the same single line otherwise.
pub fn evaluate_controls(request: &Value) -> Value {
    let audit_id = text(request, "audit_id");
    let controls = request
        .get("controls_to_test")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let mut results = Vec::with_capacity(controls.len());
    let mut material_weaknesses = 0i64;
    let mut significant_deficiencies = 0i64;
    let mut effective_controls = 0i64;

    for control in &controls {
        let deficiency = match control.get("deficiency").and_then(Value::as_str) {
            Some(value) => value.to_string(),
            None => "none".to_string(),
        };

        let (severity, effective) = match deficiency.as_str() {
            "material" => {
                material_weaknesses += 1;
                ("material_weakness", false)
            }
            "significant" => {
                significant_deficiencies += 1;
                ("significant_deficiency", false)
            }
            "none" => ("none", true),
            // Any other marker is still a deficiency; it just carries no severity.
            _ => ("none", false),
        };

        if effective {
            effective_controls += 1;
        }

        results.push(json!({
            "control_id": control.get("id").and_then(Value::as_str).unwrap_or(""),
            "control_description": control.get("description").and_then(Value::as_str).unwrap_or(""),
            "control_type": control.get("type").and_then(Value::as_str).unwrap_or("preventive"),
            "testing_result": if effective { "effective" } else { "ineffective" },
            "deficiency": deficiency,
            "severity": severity,
        }));
    }

    let clean = material_weaknesses == 0 && significant_deficiencies == 0;
    let recommendations = if material_weaknesses > 0 {
        json!([
            "Implement segregation of duties for payment processing",
            "Strengthen access controls to financial systems",
            "Enhance management review procedures",
        ])
    } else {
        json!(["Continue monitoring control effectiveness"])
    };

    json!({
        "audit_id": audit_id,
        "controls_tested": controls.len(),
        "effective_controls": effective_controls,
        "deficiencies_found": controls.len() as i64 - effective_controls,
        "material_weaknesses": material_weaknesses,
        "significant_deficiencies": significant_deficiencies,
        "control_test_results": results,
        "overall_assessment": if clean { "effective" } else { "ineffective_with_deficiencies" },
        "remediation_recommendations": recommendations,
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
    fn transfer_pricing_matches_the_service() {
        let request = json!({"company_id": "C1", "transactions": [
            {"company_id": "C1", "id": "T1", "product_service": "Widgets", "selling_entity": "US",
             "buying_entity": "ZW", "transaction_value": 100, "arm_length_range_min": 90,
             "arm_length_range_max": 110, "method": "comparable_uncontrolled_price"},
            {"company_id": "C1", "id": "T2", "product_service": "Parts", "selling_entity": "US",
             "buying_entity": "KE", "transaction_value": 80, "arm_length_range_min": 90,
             "arm_length_range_max": 120, "method": "resale_price"},
            {"company_id": "C1", "id": "T3", "product_service": "Services", "selling_entity": "ZW",
             "buying_entity": "US", "transaction_value": 130, "arm_length_range_min": 90,
             "arm_length_range_max": 120, "method": "cost_plus"}
        ]});
        let out = analyze_transfer_pricing(&request, "2026-11-03T09:00:00+00:00");
        assert_eq!(out["compliant_transactions"], json!(1));
        assert_eq!(out["non_compliant_transactions"], json!(2));
        assert_eq!(out["total_adjustment_needed"], json!(20.0));
        assert_eq!(out["transactions"][1]["adjustment_needed"], json!(10.0));
        // over-priced: the answer is the range top minus the value, so negative
        assert_eq!(out["transactions"][2]["adjustment_needed"], json!(-10.0));
        assert_eq!(
            out["transactions"][0]["arm_length_range"],
            json!([90.0, 110.0])
        );
        assert_eq!(
            out["documentation_required"][0],
            json!("Master File (OECD)")
        );
    }

    fn analytical_request() -> Value {
        json!({"audit_id": "A1", "company_id": "C1", "non_financial_factors": ["staff turnover"],
            "current_data": {"period": "2026", "revenue": 1000, "gross_profit": 400,
                             "operating_expenses": 250, "net_income": 120, "total_assets": 2000,
                             "total_liabilities": 800, "equity": 1200, "working_capital": 300},
            "prior_period_data": {"period": "2025", "revenue": 900, "gross_profit": 380,
                                  "operating_expenses": 230, "net_income": 110, "total_assets": 1900,
                                  "total_liabilities": 750, "equity": 1150, "working_capital": 280},
            "industry_benchmarks": {"gross_margin": 0.35, "operating_margin": 0.15, "roa": 0.08,
                                    "debt_equity": 1.5}})
    }

    #[test]
    fn analytical_procedures_match_the_service() {
        let expected = r#"{
            "audit_id": "A1",
            "ratio_analysis": [
                {"ratio_name": "gross_margin", "current_period": 0.4, "prior_period": 0.4222,
                 "industry_benchmark": 0.35, "variance_pct": -5.26, "status": "within_expectations"},
                {"ratio_name": "operating_margin", "current_period": 0.25, "prior_period": 0.2556,
                 "industry_benchmark": 0.15, "variance_pct": -2.17, "status": "within_expectations"},
                {"ratio_name": "return_on_assets", "current_period": 0.06, "prior_period": 0.0579,
                 "industry_benchmark": 0.08, "variance_pct": 3.64, "status": "within_expectations"},
                {"ratio_name": "debt_to_equity", "current_period": 0.6667, "prior_period": 0.6522,
                 "industry_benchmark": 1.5, "variance_pct": 2.22, "status": "within_expectations"}
            ],
            "trend_indicators": {"revenue_growth": "increasing", "profitability": "improving",
                                 "leverage": "increasing"},
            "unexpected_variances": [],
            "analytical_findings": ["No significant unexpected variances identified"],
            "conclusion": "Analytical procedures completed with high reliance on substantive testing",
            "reliance_level": "high"
        }"#;
        assert_parity(
            analyze_analytical_procedures(&analytical_request()),
            expected,
        );
    }

    #[test]
    fn a_large_variance_lands_as_a_finding_and_lowers_reliance() {
        let mut request = analytical_request();
        request["current_data"]["gross_profit"] = json!(700);
        let out = analyze_analytical_procedures(&request);
        assert_eq!(
            out["ratio_analysis"][0]["status"],
            json!("significant_variance")
        );
        assert_eq!(
            out["analytical_findings"],
            json!(["Unusual gross_margin variance of 65.8%"])
        );
        assert_eq!(out["reliance_level"], json!("moderate"));
        assert_eq!(
            out["unexpected_variances"][0]["ratio"],
            json!("gross_margin")
        );
    }
}
