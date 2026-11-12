//! Going concern assessment.
//!
//! Two details are the service's and are kept as they were. The assessment date is the fixed string
//! 2024-06-30 rather than today, so reports carry that date whatever the run. And the risk count
//! that decides the outcome looks at four indicators, not the six that are reported: arrears on
//! dividends and repayment extensions are disclosed but never move the verdict.

use serde_json::{Map, Value};

fn round2(value: f64) -> f64 {
    vimbai_common::numbers::round_dp(value, 2)
}

fn round4(value: f64) -> f64 {
    vimbai_common::numbers::round_dp(value, 4)
}

fn financial(payload: &Value, key: &str) -> f64 {
    payload
        .get("financial_data")
        .and_then(|f| f.get(key))
        .and_then(Value::as_f64)
        .unwrap_or(0.0)
}

fn cash(payload: &Value, key: &str) -> f64 {
    payload
        .get("cash_flows")
        .and_then(|f| f.get(key))
        .and_then(Value::as_f64)
        .unwrap_or(0.0)
}

fn covenants(payload: &Value, flag: &str) -> bool {
    payload
        .get("debt_covenants")
        .and_then(Value::as_array)
        .map(|rows| {
            rows.iter()
                .any(|row| row.get(flag).and_then(Value::as_bool).unwrap_or(false))
        })
        .unwrap_or(false)
}

fn non_empty(payload: &Value, key: &str) -> bool {
    payload
        .get(key)
        .and_then(Value::as_array)
        .map(|rows| !rows.is_empty())
        .unwrap_or(false)
}

/// `POST /assess`.
pub fn assess(payload: &Value) -> Result<Value, String> {
    let current_assets = financial(payload, "current_assets");
    let non_current_assets = financial(payload, "non_current_assets");
    let current_liabilities = financial(payload, "current_liabilities");
    let non_current_liabilities = financial(payload, "non_current_liabilities");

    let assets = current_assets + non_current_assets;
    let liabilities = current_liabilities + non_current_liabilities;
    let working_capital = current_assets - current_liabilities;
    let current_ratio = if current_liabilities == 0.0 {
        current_assets
    } else {
        current_assets / current_liabilities
    };
    let debt_ratio = if assets == 0.0 {
        1.0
    } else {
        liabilities / assets
    };
    let liquidity_score = ((current_ratio - 0.5) * 100.0).clamp(0.0, 100.0);
    let solvency_score = ((1.0 - debt_ratio) * 100.0).clamp(0.0, 100.0);

    let debt: f64 = payload
        .get("debt_covenants")
        .and_then(Value::as_array)
        .map(|rows| {
            rows.iter()
                .map(|row| row.get("amount").and_then(Value::as_f64).unwrap_or(0.0))
                .sum()
        })
        .unwrap_or(0.0);
    let cash_flow_adequacy = if debt != 0.0 {
        (cash(payload, "operating") + cash(payload, "financing")) / (debt + 1.0)
    } else {
        0.5
    };

    let negative_working_capital = working_capital < 0.0;
    let borrowing_agreement_breaches = covenants(payload, "breach");
    let covenant_violations = covenants(payload, "violation");
    let going_concern_modifications = financial(payload, "going_concern_modifications") > 0.0;
    let arrears_on_dividends = financial(payload, "dividend_arrears") > 0.0;
    let loan_repayment_extensions = financial(payload, "extensions_granted") > 0.0;

    let risk_count = [
        negative_working_capital,
        borrowing_agreement_breaches,
        covenant_violations,
        going_concern_modifications,
    ]
    .iter()
    .filter(|flag| **flag)
    .count();

    let (result, material_uncertainty) = if risk_count >= 3 || liquidity_score < 30.0 {
        ("high_risk", true)
    } else if risk_count >= 1 || liquidity_score < 50.0 {
        ("medium_risk", false)
    } else {
        ("low_risk", false)
    };

    let mut mitigating = Vec::new();
    if non_empty(payload, "management_plans") {
        mitigating.push(Value::String(
            "Management has prepared mitigation plans".into(),
        ));
    }
    if non_empty(payload, "financing_arrangements") {
        mitigating.push(Value::String(
            "Committed financing facilities in place".into(),
        ));
    }
    if cash_flow_adequacy > 0.0 {
        mitigating.push(Value::String("Positive cash flow from operations".into()));
    }

    let mut out = Map::new();
    for key in ["company_id", "audit_id"] {
        out.insert(key.into(), payload.get(key).cloned().unwrap_or(Value::Null));
    }
    out.insert("assessment_date".into(), Value::String("2024-06-30".into()));
    out.insert(
        "risk_indicators".into(),
        serde_json::json!({
            "negative_working_capital": negative_working_capital,
            "borrowing_agreement_breaches": borrowing_agreement_breaches,
            "covenant_violations": covenant_violations,
            "going_concern_modifications": going_concern_modifications,
            "arrears_on_dividends": arrears_on_dividends,
            "loan_repayment_extensions": loan_repayment_extensions,
        }),
    );
    out.insert(
        "liquidity_score".into(),
        Value::from(round2(liquidity_score)),
    );
    out.insert("solvency_score".into(), Value::from(round2(solvency_score)));
    out.insert(
        "cash_flow_adequacy".into(),
        Value::from(round4(cash_flow_adequacy)),
    );
    out.insert("assessment_result".into(), Value::String(result.into()));
    out.insert(
        "material_uncertainty".into(),
        Value::Bool(material_uncertainty),
    );
    out.insert(
        "mitigating_factors".into(),
        if mitigating.is_empty() {
            serde_json::json!(["No specific mitigating factors identified"])
        } else {
            Value::Array(mitigating)
        },
    );
    out.insert(
        "conclusions".into(),
        serde_json::json!({
            "going_concern": if result == "high_risk" {
                "Basis for concern exists"
            } else {
                "No significant doubts"
            },
            "disclosure": if material_uncertainty {
                "Material uncertainty disclosure required"
            } else {
                "Standard disclosure"
            },
            "opinion_impact": if material_uncertainty {
                "Qualified or emphasis of matter"
            } else {
                "Unqualified"
            },
        }),
    );
    Ok(Value::Object(out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn payload() -> Value {
        json!({
            "company_id": "c-1", "audit_id": "gc-1",
            "financial_data": {
                "current_assets": 400_000.0, "non_current_assets": 600_000.0,
                "current_liabilities": 500_000.0, "non_current_liabilities": 700_000.0,
                "going_concern_modifications": 1.0
            },
            "debt_covenants": [
                {"amount": 200_000.0, "breach": true, "violation": false},
                {"amount": 100_000.0, "breach": false, "violation": true}
            ],
            "cash_flows": {"operating": 80_000.0, "financing": -20_000.0},
            "management_plans": ["Cost reduction"], "financing_arrangements": ["Overdraft"]
        })
    }

    #[test]
    fn three_indicators_mean_material_uncertainty() {
        let out = assess(&payload()).unwrap();
        assert_eq!(out["assessment_result"], json!("high_risk"));
        assert_eq!(out["material_uncertainty"], json!(true));
        assert_eq!(
            out["conclusions"]["opinion_impact"],
            json!("Qualified or emphasis of matter")
        );
    }

    #[test]
    fn the_assessment_date_is_a_fixed_string() {
        let out = assess(&payload()).unwrap();
        assert_eq!(out["assessment_date"], json!("2024-06-30"));
    }

    #[test]
    fn arrears_and_extensions_are_disclosed_but_do_not_count() {
        let out = assess(&json!({
            "company_id": "c-2", "audit_id": "gc-2",
            "financial_data": {
                "current_assets": 900_000.0, "non_current_assets": 100_000.0,
                "current_liabilities": 200_000.0, "non_current_liabilities": 100_000.0,
                "dividend_arrears": 1.0, "extensions_granted": 1.0
            },
            "debt_covenants": [], "cash_flows": {"operating": 50_000.0}
        }))
        .unwrap();
        assert_eq!(out["risk_indicators"]["arrears_on_dividends"], json!(true));
        assert_eq!(out["assessment_result"], json!("low_risk"));
    }

    #[test]
    fn no_debt_leaves_cash_flow_adequacy_at_half() {
        let out = assess(&json!({
            "company_id": "c-3", "audit_id": "gc-3",
            "financial_data": {"current_assets": 100.0, "current_liabilities": 20.0}
        }))
        .unwrap();
        assert_eq!(out["cash_flow_adequacy"], json!(0.5));
        assert_eq!(out["solvency_score"], json!(80.0));
    }
}
