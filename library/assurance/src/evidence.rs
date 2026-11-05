//! Audit evidence sufficiency and appropriateness.
//!
//! The service scored evidence by counting flags, not by weighting it: an item is as good as its
//! `sufficiency` and `appropriateness` booleans say. `reliability_factors` and
//! `required_evidence_types` are accepted and never used - the thresholds are the fixed 70% and 80%
//! of items below, whatever the assertion.
//!
//! `evidence_types_used` was built from a Python set, so its order changed between runs. This port
//! returns the types in the order they first appear, which is stable and equally acceptable to a
//! caller that treats the field as a set.

use serde_json::{Map, Value};

fn round2(value: f64) -> f64 {
    vimbai_common::numbers::round_dp(value, 2)
}

fn string(item: &Value, key: &str) -> Value {
    item.get(key)
        .cloned()
        .unwrap_or(Value::String(String::new()))
}

/// `POST /evaluate`.
pub fn evaluate(payload: &Value) -> Result<Value, String> {
    let items = payload
        .get("evidence_items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let total = items.len();

    let mut sufficiency_count = 0usize;
    let mut appropriateness_count = 0usize;
    let mut shaped = Vec::with_capacity(total);
    let mut types: Vec<String> = Vec::new();

    for item in &items {
        let sufficiency = item
            .get("sufficiency")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let appropriateness = item
            .get("appropriateness")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if sufficiency {
            sufficiency_count += 1;
        }
        if appropriateness {
            appropriateness_count += 1;
        }
        let kind = item
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        if !types.contains(&kind) {
            types.push(kind.clone());
        }
        shaped.push(serde_json::json!({
            "evidence_id": string(item, "id"),
            "source": string(item, "source"),
            "type": kind,
            "description": string(item, "description"),
            "obtained_date": string(item, "date"),
            "sufficiency": sufficiency,
            "appropriateness": appropriateness,
        }));
    }

    let reliability = if total == 0 {
        0.0
    } else {
        (sufficiency_count + appropriateness_count) as f64 / (2.0 * total as f64)
    };

    let sufficient = total > 0 && sufficiency_count as f64 >= total as f64 * 0.7;
    let appropriate = total > 0 && appropriateness_count as f64 >= total as f64 * 0.8;

    let mut additional = Vec::new();
    if !sufficient {
        additional.push(Value::String("Extend sample sizes".into()));
    }
    if !appropriate {
        additional.push(Value::String(
            "Obtain corroborating evidence from independent sources".into(),
        ));
    }

    let mut out = Map::new();
    for key in ["audit_id", "assertion"] {
        out.insert(key.into(), payload.get(key).cloned().unwrap_or(Value::Null));
    }
    out.insert("evidence_gathered".into(), Value::from(total as u64));
    out.insert(
        "evidence_types_used".into(),
        Value::Array(types.into_iter().map(Value::String).collect()),
    );
    out.insert(
        "sufficiency_assessment".into(),
        Value::String(
            if sufficient {
                "sufficient"
            } else {
                "insufficient"
            }
            .into(),
        ),
    );
    out.insert(
        "appropriateness_assessment".into(),
        Value::String(
            if appropriate {
                "appropriate"
            } else {
                "concerns_noted"
            }
            .into(),
        ),
    );
    out.insert("reliability_score".into(), Value::from(round2(reliability)));
    out.insert("evidence_items".into(), Value::Array(shaped));
    out.insert(
        "conclusion".into(),
        Value::String(
            if sufficient && appropriate {
                "Evidence supports assertion"
            } else {
                "Evidence requires supplementation"
            }
            .into(),
        ),
    );
    out.insert(
        "additional_evidence_required".into(),
        if additional.is_empty() {
            serde_json::json!(["No additional evidence required"])
        } else {
            Value::Array(additional)
        },
    );
    Ok(Value::Object(out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn payload() -> Value {
        json!({
            "audit_id": "ae-1", "assertion": "existence",
            "required_evidence_types": ["invoice"], "reliability_factors": {"bank": "high"},
            "evidence_items": [
                {"id": "e1", "source": "supplier", "type": "invoice", "date": "2026-01-10",
                 "sufficiency": true, "appropriateness": true},
                {"id": "e2", "source": "bank", "type": "bank_statement", "date": "2026-01-12",
                 "sufficiency": true, "appropriateness": false},
                {"id": "e3", "source": "internal", "type": "ledger", "date": "2026-01-15",
                 "sufficiency": false, "appropriateness": true}
            ]
        })
    }

    #[test]
    fn two_of_three_flags_each_fails_both_thresholds() {
        let out = evaluate(&payload()).unwrap();
        assert_eq!(out["evidence_gathered"], json!(3));
        assert_eq!(out["sufficiency_assessment"], json!("insufficient"));
        assert_eq!(out["appropriateness_assessment"], json!("concerns_noted"));
        assert_eq!(out["reliability_score"], json!(0.67));
    }

    #[test]
    fn types_are_listed_once_in_the_order_they_appear() {
        let out = evaluate(&payload()).unwrap();
        assert_eq!(
            out["evidence_types_used"],
            json!(["invoice", "bank_statement", "ledger"])
        );
    }

    #[test]
    fn missing_flags_count_as_no_rather_than_as_silence() {
        let out = evaluate(&json!({
            "audit_id": "ae-2", "assertion": "cutoff",
            "evidence_items": [{"id": "e1", "type": "invoice"}]
        }))
        .unwrap();
        assert_eq!(out["reliability_score"], json!(0.0));
        assert_eq!(
            out["additional_evidence_required"],
            json!([
                "Extend sample sizes",
                "Obtain corroborating evidence from independent sources"
            ])
        );
    }

    #[test]
    fn no_evidence_at_all_is_insufficient_without_dividing_by_zero() {
        let out = evaluate(&json!({"audit_id": "ae-3", "assertion": "existence"})).unwrap();
        assert_eq!(out["reliability_score"], json!(0.0));
        assert_eq!(
            out["conclusion"],
            json!("Evidence requires supplementation")
        );
    }
}
