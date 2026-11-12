//! Provisions and contingencies: the IAS 37 sweep as the service performed it.
//!
//! A provision is recognised when its best estimate is above zero and it is probable. The `probable`
//! flag defaults to true when absent, so a caller who says nothing about likelihood gets recognition
//! rather than silence - that default is the service's, and it is kept.
//!
//! Two presentational choices matter to whoever reads the output: an empty list of unrecognised
//! provisions is replaced with a single message object, so the field is never empty; and
//! `legal_advice_available` is required by the request but never consulted when deciding
//! disclosure completeness.

use serde_json::{Map, Value};

fn round2(value: f64) -> f64 {
    vimbai_common::numbers::round_dp(value, 2)
}

fn number(item: &Value, key: &str) -> f64 {
    item.get(key).and_then(Value::as_f64).unwrap_or(0.0)
}

fn string(item: &Value, key: &str) -> Value {
    item.get(key)
        .cloned()
        .unwrap_or(Value::String(String::new()))
}

/// `POST /assess` of the provisions and contingencies service.
pub fn assess(payload: &Value) -> Result<Value, String> {
    let provisions = payload
        .get("provisions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let contingencies = payload
        .get("contingencies")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let uncertain_taxes = payload
        .get("tax_uncertainties")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let mut items = Vec::with_capacity(provisions.len());
    let mut total = 0.0;
    let mut recognised = 0u32;

    for provision in &provisions {
        let best = number(provision, "best_estimate");
        let possible = number(provision, "possible_outcome");
        let probable = provision
            .get("probable")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let status = if best > 0.0 && probable {
            recognised += 1;
            total += best;
            "recognized"
        } else {
            "not_recognized"
        };

        items.push(serde_json::json!({
            "provision_id": string(provision, "id"),
            "category": string(provision, "category"),
            "description": string(provision, "description"),
            "carrying_amount": number(provision, "carrying_amount"),
            "best_estimate": best,
            "possible_outcome": possible,
            "recognition_status": status,
        }));
    }

    let unrecognised: Vec<Value> = items
        .iter()
        .filter(|item| item["recognition_status"] == "not_recognized")
        .map(|item| {
            serde_json::json!({
                "id": item["provision_id"],
                "description": item["description"],
                "possible_amount": item["possible_outcome"],
            })
        })
        .collect();

    let count_of = |kind: &str| {
        contingencies
            .iter()
            .filter(|entry| entry.get("type").and_then(Value::as_str) == Some(kind))
            .count()
    };

    let current_tax: f64 = uncertain_taxes.iter().map(|t| number(t, "current")).sum();
    let deferred_tax: f64 = uncertain_taxes.iter().map(|t| number(t, "deferred")).sum();
    let uncertain_positions = uncertain_taxes
        .iter()
        .filter(|t| t.get("uncertain").and_then(Value::as_bool).unwrap_or(false))
        .count();

    let mut out = Map::new();
    out.insert(
        "company_id".into(),
        payload.get("company_id").cloned().unwrap_or(Value::Null),
    );
    out.insert(
        "audit_id".into(),
        payload.get("audit_id").cloned().unwrap_or(Value::Null),
    );
    out.insert("provisions_recognized".into(), Value::from(recognised));
    out.insert("total_provision_amount".into(), Value::from(round2(total)));
    out.insert(
        "contingent_liabilities".into(),
        Value::from(count_of("liability") as u64),
    );
    out.insert(
        "contingent_assets".into(),
        Value::from(count_of("asset") as u64),
    );
    out.insert(
        "unrecognised_provisions".into(),
        if unrecognised.is_empty() {
            serde_json::json!([{"message": "No unrecognised provisions"}])
        } else {
            Value::Array(unrecognised)
        },
    );
    out.insert("provision_items".into(), Value::Array(items));
    out.insert(
        "tax_provisions".into(),
        serde_json::json!({
            "current_tax": current_tax,
            "deferred_tax": deferred_tax,
            "uncertain_positions": uncertain_positions,
        }),
    );
    out.insert(
        "disclosure_completeness".into(),
        Value::String(
            if recognised > 0 {
                "Complete"
            } else {
                "Review required"
            }
            .into(),
        ),
    );
    out.insert(
        "recommendations".into(),
        serde_json::json!([
            "Ensure provisions meet recognition criteria under IAS 37",
            "Review contingent liabilities for possible recognition",
            "Document estimation techniques for provisions",
        ]),
    );
    Ok(Value::Object(out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn an_absent_probable_flag_means_probable() {
        let out = assess(&json!({
            "company_id": "c-1", "audit_id": "a-1",
            "provisions": [{"id": "p1", "best_estimate": 500.0}],
            "contingencies": [], "tax_uncertainties": []
        }))
        .unwrap();
        assert_eq!(out["provisions_recognized"], json!(1));
        assert_eq!(out["total_provision_amount"], json!(500.0));
    }

    #[test]
    fn a_zero_best_estimate_is_never_recognised_even_when_probable() {
        let out = assess(&json!({
            "company_id": "c-1", "audit_id": "a-1",
            "provisions": [{"id": "p1", "best_estimate": 0.0, "probable": true}],
            "contingencies": [], "tax_uncertainties": []
        }))
        .unwrap();
        assert_eq!(out["provisions_recognized"], json!(0));
        assert_eq!(out["disclosure_completeness"], json!("Review required"));
    }

    #[test]
    fn no_unrecognised_provisions_still_reads_as_a_list_with_a_message() {
        let out = assess(&json!({
            "company_id": "c-1", "audit_id": "a-1",
            "provisions": [{"id": "p1", "best_estimate": 100.0}],
            "contingencies": [], "tax_uncertainties": []
        }))
        .unwrap();
        assert_eq!(
            out["unrecognised_provisions"],
            json!([{"message": "No unrecognised provisions"}])
        );
    }

    #[test]
    fn contingencies_are_counted_by_kind() {
        let out = assess(&json!({
            "company_id": "c-1", "audit_id": "a-1", "provisions": [],
            "contingencies": [{"type": "liability"}, {"type": "liability"}, {"type": "asset"}],
            "tax_uncertainties": [{"current": 10.0, "deferred": 5.0, "uncertain": true}]
        }))
        .unwrap();
        assert_eq!(out["contingent_liabilities"], json!(2));
        assert_eq!(out["contingent_assets"], json!(1));
        assert_eq!(out["tax_provisions"]["current_tax"], json!(10.0));
        assert_eq!(out["tax_provisions"]["uncertain_positions"], json!(1));
    }
}
