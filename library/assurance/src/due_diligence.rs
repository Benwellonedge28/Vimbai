//! Due diligence scoring: a running risk count, not a weighted model.
//!
//! Each finding adds a fixed number of points - 20 for a loss, 15 for heavy leverage, 10 for customer
//! concentration, 5 per legal or regulatory item - and the rating is a threshold on that total. A
//! deal with a single legal issue and one red flag can therefore beat a profitable, unlevered target
//! with three small concerns, which is worth knowing before treating the rating as a judgement.
//!
//! `assessment_date` is not minted here. The service stamped the wall clock, which a pure function
//! cannot reproduce; a caller that wants the field supplies it and it is echoed back.

use serde_json::{Map, Value};

fn round4(value: f64) -> f64 {
    vimbai_common::numbers::round_dp(value, 4)
}

fn metric(payload: &Value, key: &str) -> Option<f64> {
    payload
        .get("financial_metrics")
        .and_then(|f| f.get(key))
        .and_then(Value::as_f64)
}

fn ratio(payload: &Value, numerator: &str, denominator: &str, when_absent: f64) -> f64 {
    let top = metric(payload, numerator).unwrap_or(0.0);
    match metric(payload, denominator) {
        Some(bottom) if bottom != 0.0 => top / bottom,
        _ => when_absent,
    }
}

/// `POST /assess`.
pub fn assess(payload: &Value) -> Result<Value, String> {
    let profit_margin = ratio(payload, "net_income", "revenue", 0.0);
    let debt_ratio = ratio(payload, "total_debt", "total_assets", 0.0);
    let current_ratio = ratio(payload, "current_assets", "current_liabilities", 0.0);
    let roe = ratio(payload, "net_income", "equity", 0.0);

    let customer_concentration = payload
        .get("customer_concentration")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let key_person_risk = payload
        .get("key_person_risk")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let litigation_risk = payload
        .get("litigation_risk")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let environmental_risk = payload
        .get("environmental_risk")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let legal_issues = payload
        .get("legal_issues")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    let regulatory_flags = payload
        .get("regulatory_flags")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);

    let mut score = 0i64;
    let mut red = Vec::new();
    let mut green = Vec::new();

    if profit_margin < 0.0 {
        score += 20;
        red.push(Value::String("Negative profit margin detected".into()));
    } else {
        green.push(Value::String("Profitable operations".into()));
    }
    if debt_ratio > 0.7 {
        score += 15;
        red.push(Value::String("High leverage ratio".into()));
    } else if debt_ratio < 0.4 {
        green.push(Value::String("Conservative capital structure".into()));
    }
    if customer_concentration > 0.3 {
        score += 10;
        red.push(Value::String(format!(
            "High customer concentration: {:.1}%",
            customer_concentration * 100.0
        )));
    } else {
        green.push(Value::String("Diversified customer base".into()));
    }
    if key_person_risk {
        score += 10;
        red.push(Value::String("Key person dependency identified".into()));
    }
    if legal_issues > 0 {
        score += legal_issues as i64 * 5;
        red.push(Value::String(format!(
            "{legal_issues} legal issues identified"
        )));
    }
    if regulatory_flags > 0 {
        score += regulatory_flags as i64 * 5;
        red.push(Value::String(format!(
            "{regulatory_flags} regulatory concerns"
        )));
    }
    if environmental_risk {
        score += 15;
        red.push(Value::String("Environmental compliance risk".into()));
    }
    if litigation_risk > 0.5 {
        score += 10;
        red.push(Value::String("High litigation exposure".into()));
    }

    let rating = if score > 50 {
        "HIGH"
    } else if score > 25 {
        "MEDIUM"
    } else {
        "LOW"
    };
    if score < 20 {
        green.push(Value::String("Low overall risk profile".into()));
    }

    let mut recommendations = Vec::new();
    if score > 40 {
        recommendations.push(Value::String(
            "Consider detailed investigation of flagged issues before proceeding".into(),
        ));
    }
    if customer_concentration > 0.3 {
        recommendations.push(Value::String(
            "Develop customer diversification plan as part of integration".into(),
        ));
    }
    if key_person_risk {
        recommendations.push(Value::String(
            "Implement retention agreements and succession planning".into(),
        ));
    }

    let mut out = Map::new();
    for key in ["target_id", "acquirer_id"] {
        out.insert(key.into(), payload.get(key).cloned().unwrap_or(Value::Null));
    }
    if let Some(date) = payload.get("assessment_date") {
        out.insert("assessment_date".into(), date.clone());
    }
    out.insert(
        "financial_health".into(),
        serde_json::json!({
            "profit_margin": round4(profit_margin),
            "debt_ratio": round4(debt_ratio),
            "current_ratio": round4(current_ratio),
            "roe": round4(roe),
            "revenue_growth": metric(payload, "revenue_growth").unwrap_or(0.0),
            "score": if profit_margin > 0.1 && debt_ratio < 0.5 { "Strong" } else { "Weak" },
        }),
    );
    out.insert(
        "operational_risk".into(),
        serde_json::json!({
            "customer_concentration": customer_concentration,
            "supplier_concentration": payload
                .get("supplier_concentration")
                .cloned()
                .unwrap_or(Value::from(0.0)),
            "key_person_risk": key_person_risk,
            "cybersecurity_risk": payload
                .get("cybersecurity_risk")
                .cloned()
                .unwrap_or(Value::Null),
        }),
    );
    out.insert(
        "legal_compliance".into(),
        serde_json::json!({
            "legal_issues_count": legal_issues,
            "regulatory_flags_count": regulatory_flags,
            "litigation_risk": litigation_risk,
            "environmental_risk": environmental_risk,
        }),
    );
    out.insert("overall_risk_score".into(), Value::from(score));
    out.insert("risk_rating".into(), Value::String(rating.into()));
    out.insert("red_flags".into(), Value::Array(red));
    out.insert("green_flags".into(), Value::Array(green));
    out.insert("recommendations".into(), Value::Array(recommendations));
    Ok(Value::Object(out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn payload() -> Value {
        json!({
            "target_id": "t-1", "acquirer_id": "a-1",
            "financial_metrics": {
                "revenue": 10_000_000.0, "net_income": 800_000.0, "total_debt": 5_000_000.0,
                "total_assets": 9_000_000.0, "current_assets": 3_000_000.0,
                "current_liabilities": 2_000_000.0, "equity": 4_000_000.0, "revenue_growth": 0.12
            },
            "legal_issues": ["Outstanding litigation"], "regulatory_flags": ["Data notice"],
            "customer_concentration": 0.45, "supplier_concentration": 0.2,
            "key_person_risk": true, "litigation_risk": 0.6,
            "environmental_risk": false, "cybersecurity_risk": "medium"
        })
    }

    #[test]
    fn findings_add_fixed_points() {
        let out = assess(&payload()).unwrap();
        // concentration 10 + key person 10 + 1 legal 5 + 1 regulatory 5 + litigation 10.
        // Leverage is 0.56, between the 0.4 bonus and the 0.7 penalty, so it scores nothing.
        assert_eq!(out["overall_risk_score"], json!(40));
        assert_eq!(out["risk_rating"], json!("MEDIUM"));
    }

    #[test]
    fn ratios_are_rounded_for_display_only() {
        let out = assess(&payload()).unwrap();
        assert_eq!(out["financial_health"]["profit_margin"], json!(0.08));
        assert_eq!(out["financial_health"]["debt_ratio"], json!(0.5556));
        assert_eq!(out["financial_health"]["score"], json!("Weak"));
    }

    #[test]
    fn concentration_is_reported_to_one_decimal() {
        let out = assess(&payload()).unwrap();
        assert!(out["red_flags"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f == "High customer concentration: 45.0%"));
    }

    #[test]
    fn a_clean_target_carries_a_low_profile_flag_and_no_recommendations() {
        let out = assess(&json!({
            "target_id": "t-2", "acquirer_id": "a-1",
            "financial_metrics": {"revenue": 1_000.0, "net_income": 200.0,
                                  "total_debt": 100.0, "total_assets": 1_000.0,
                                  "current_assets": 400.0, "current_liabilities": 200.0,
                                  "equity": 800.0},
            "legal_issues": [], "regulatory_flags": [],
            "customer_concentration": 0.1, "key_person_risk": false, "litigation_risk": 0.0
        }))
        .unwrap();
        assert_eq!(out["risk_rating"], json!("LOW"));
        assert!(out["green_flags"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f == "Low overall risk profile"));
        assert_eq!(out["recommendations"], json!([]));
    }

    #[test]
    fn no_metrics_does_not_divide_by_zero() {
        let out = assess(&json!({"target_id": "t-3", "acquirer_id": "a-1"})).unwrap();
        assert_eq!(out["financial_health"]["profit_margin"], json!(0.0));
        assert!(out.get("assessment_date").is_none());
    }
}
