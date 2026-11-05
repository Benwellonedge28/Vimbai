//! Operational risk, as the service assessed it.
//!
//! The loss distribution is the simple shape the service used, not a fitted one: value at risk is
//! the largest single loss times 2.5, and regulatory capital is that figure times the Basel beta
//! of 12.5. Keeping the formula unchanged matters more than improving it here, because callers
//! already hold numbers produced by it.
//!
//! A key risk indicator is graded by its green and amber thresholds only; the red threshold is
//! carried in the request but never consulted, which is what the Python did.

use serde_json::{Map, Value};

fn number(value: Option<&Value>) -> f64 {
    match value {
        Some(Value::Number(n)) => n.as_f64().unwrap_or(0.0),
        Some(Value::String(s)) => s.trim().parse().unwrap_or(0.0),
        _ => 0.0,
    }
}

/// `POST /assess` of the operational risk service.
pub fn assess(payload: &Value) -> Result<Value, String> {
    let losses = payload
        .get("loss_events")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let kris = payload
        .get("kris")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let amounts: Vec<f64> = losses
        .iter()
        .map(|e| number(e.get("loss_amount")))
        .collect();
    let total: f64 = amounts.iter().sum();
    let count = amounts.len();
    let average = if count == 0 {
        0.0
    } else {
        total / count as f64
    };
    let max = amounts
        .iter()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max)
        .max(0.0);

    let var_99_9 = max * 2.5;
    let beta_factor = 12.5;
    let regulatory_capital = var_99_9 * beta_factor;

    let mut kri_status = Map::new();
    let mut reds = 0u32;
    for kri in &kris {
        let name = kri
            .get("indicator_name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let current = number(kri.get("current_value"));
        let green = number(kri.get("threshold_green"));
        let amber = number(kri.get("threshold_amber"));
        let status = if current <= green {
            "GREEN"
        } else if current <= amber {
            "AMBER"
        } else {
            reds += 1;
            "RED"
        };
        kri_status.insert(name, Value::String(status.to_string()));
    }

    let rating = if reds == 0 {
        "LOW"
    } else if reds <= 2 {
        "MEDIUM"
    } else {
        "HIGH"
    };

    let mut out = Map::new();
    out.insert(
        "company_id".into(),
        payload.get("company_id").cloned().unwrap_or(Value::Null),
    );
    out.insert("total_losses".into(), serde_json::json!(total));
    out.insert("average_loss".into(), serde_json::json!(average));
    out.insert("max_loss".into(), serde_json::json!(max));
    out.insert("loss_count".into(), serde_json::json!(count));
    out.insert("var_99_9".into(), serde_json::json!(var_99_9));
    out.insert(
        "regulatory_capital".into(),
        serde_json::json!(regulatory_capital),
    );
    out.insert("kri_status".into(), Value::Object(kri_status));
    out.insert("risk_rating".into(), Value::String(rating.to_string()));
    Ok(Value::Object(out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_book_with_no_losses_has_no_capital_requirement() {
        let out = assess(&json!({"company_id": "c-1", "loss_events": [], "kris": []})).unwrap();
        assert_eq!(out["total_losses"], json!(0.0));
        assert_eq!(out["max_loss"], json!(0.0));
        assert_eq!(out["regulatory_capital"], json!(0.0));
        assert_eq!(out["risk_rating"], json!("LOW"));
    }

    #[test]
    fn the_red_threshold_is_carried_but_not_used() {
        // current sits above amber but the indicator is still graded by the amber line only
        let out = assess(&json!({
            "company_id": "c-1",
            "loss_events": [],
            "kris": [{
                "indicator_name": "Downtime", "current_value": 15.0,
                "threshold_green": 5.0, "threshold_amber": 10.0, "threshold_red": 12.0
            }]
        }))
        .unwrap();
        assert_eq!(out["kri_status"]["Downtime"], json!("RED"));
        assert_eq!(out["risk_rating"], json!("MEDIUM"));
    }

    #[test]
    fn three_red_indicators_raise_the_rating() {
        let mut kris = Vec::new();
        for name in ["a", "b", "c"] {
            kris.push(json!({
                "indicator_name": name, "current_value": 99.0,
                "threshold_green": 1.0, "threshold_amber": 2.0, "threshold_red": 3.0
            }));
        }
        let out = assess(&json!({"company_id": "c-1", "loss_events": [], "kris": kris})).unwrap();
        assert_eq!(out["risk_rating"], json!("HIGH"));
    }

    #[test]
    fn capital_is_the_largest_loss_scaled_by_both_factors() {
        let out = assess(&json!({
            "company_id": "c-1",
            "loss_events": [{"loss_amount": 1000.0}, {"loss_amount": 400.0}],
            "kris": []
        }))
        .unwrap();
        assert_eq!(out["var_99_9"], json!(2500.0));
        assert_eq!(out["regulatory_capital"], json!(31250.0));
        assert_eq!(out["average_loss"], json!(700.0));
    }
}
