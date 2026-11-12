//! Business combination: purchase price allocation and goodwill.
//!
//! Only assets typed `tangible` or `intangible` count towards identifiable assets, and liabilities
//! are summed as absolute values - so a liability entered as a positive number and one entered as a
//! negative both reduce net assets. `acquisition_costs` is reported but never deducted from goodwill,
//! which is the opposite of IFRS 3 for transaction costs; the figure is carried, not applied.
//!
//! Goodwill sits in consideration, so a bargain purchase is computed separately as net assets above
//! consideration rather than as negative goodwill. `goodwill_recognized` suppresses the goodwill line
//! from the allocation without changing the reported goodwill figure.
//!
//! The allocation percentage divides by total assets with a floor of 1, so an empty entity does not
//! produce infinities.

use serde_json::{Map, Value};

fn round1(value: f64) -> f64 {
    vimbai_common::numbers::round_dp(value, 1)
}

fn round2(value: f64) -> f64 {
    vimbai_common::numbers::round_dp(value, 2)
}

fn field(payload: &Value, key: &str) -> f64 {
    payload.get(key).and_then(Value::as_f64).unwrap_or(0.0)
}

/// `POST /acquire`.
pub fn acquire(payload: &Value) -> Result<Value, String> {
    let assets = payload
        .get("identifiable_assets")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let kind_of = |asset: &Value| {
        asset
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string()
    };
    let value_of = |asset: &Value| {
        asset
            .get("fair_value")
            .and_then(Value::as_f64)
            .unwrap_or(0.0)
    };

    let total_assets: f64 = assets
        .iter()
        .filter(|a| matches!(kind_of(a).as_str(), "tangible" | "intangible"))
        .map(value_of)
        .sum();
    let total_liabilities: f64 = assets
        .iter()
        .filter(|a| kind_of(a) == "liability")
        .map(|a| value_of(a).abs())
        .sum();
    let net_identifiable = total_assets - total_liabilities;

    let purchase_price = field(payload, "purchase_price");
    let contingent = field(payload, "contingent_consideration");
    let nci = field(payload, "non_controlling_interest");
    let consideration = purchase_price + contingent + nci;
    let goodwill = consideration - net_identifiable;
    let bargain = (net_identifiable - consideration).max(0.0);
    let recognize_goodwill = payload
        .get("goodwill_recognized")
        .and_then(Value::as_bool)
        .unwrap_or(true);

    let divisor = if total_assets > 1.0 {
        total_assets
    } else {
        1.0
    };
    let mut allocation: Vec<Value> = assets
        .iter()
        .map(|asset| {
            serde_json::json!({
                "asset": asset.get("name").cloned().unwrap_or(Value::Null),
                "type": kind_of(asset),
                "fair_value": round2(value_of(asset)),
                "allocation_pct": round1(value_of(asset).abs() / divisor * 100.0),
            })
        })
        .collect();
    if goodwill > 0.0 && recognize_goodwill {
        allocation.push(serde_json::json!({
            "asset": "Goodwill",
            "type": "intangible",
            "fair_value": round2(goodwill),
            "allocation_pct": 0,
        }));
    }
    if bargain > 0.0 {
        allocation.push(serde_json::json!({
            "asset": "Bargain Purchase Gain",
            "type": "income",
            "fair_value": round2(bargain),
            "allocation_pct": 0,
        }));
    }

    let mut out = Map::new();
    for key in ["company_id", "acquirer", "acquiree"] {
        out.insert(key.into(), payload.get(key).cloned().unwrap_or(Value::Null));
    }
    out.insert("purchase_price".into(), Value::from(round2(purchase_price)));
    out.insert(
        "total_identifiable_assets".into(),
        Value::from(round2(total_assets)),
    );
    out.insert(
        "total_identifiable_liabilities".into(),
        Value::from(round2(total_liabilities)),
    );
    out.insert(
        "net_identifiable_assets".into(),
        Value::from(round2(net_identifiable)),
    );
    out.insert("goodwill".into(), Value::from(round2(goodwill)));
    out.insert("bargain_purchase".into(), Value::from(round2(bargain)));
    out.insert(
        "contingent_consideration".into(),
        Value::from(round2(contingent)),
    );
    out.insert(
        "acquisition_costs".into(),
        Value::from(round2(field(payload, "acquisition_costs"))),
    );
    out.insert("non_controlling_interest".into(), Value::from(round2(nci)));
    out.insert("purchase_price_allocation".into(), Value::Array(allocation));
    Ok(Value::Object(out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn deal() -> Value {
        json!({
            "company_id": "c-1", "acquirer": "Vimbai Ltd", "acquiree": "Beta",
            "purchase_price": 5_000_000.0, "goodwill_recognized": true,
            "identifiable_assets": [
                {"name": "Plant", "type": "tangible", "fair_value": 3_000_000.0},
                {"name": "Brand", "type": "intangible", "fair_value": 1_200_000.0},
                {"name": "Loan", "type": "liability", "fair_value": -800_000.0}
            ],
            "contingent_consideration": 200_000.0,
            "acquisition_costs": 50_000.0,
            "non_controlling_interest": 100_000.0
        })
    }

    #[test]
    fn liabilities_enter_as_absolute_values() {
        let out = acquire(&deal()).unwrap();
        assert_eq!(out["total_identifiable_assets"], json!(4_200_000.0));
        assert_eq!(out["total_identifiable_liabilities"], json!(800_000.0));
        assert_eq!(out["net_identifiable_assets"], json!(3_400_000.0));
    }

    #[test]
    fn goodwill_counts_contingent_consideration_and_the_minority_stake() {
        let out = acquire(&deal()).unwrap();
        // 5.0m + 0.2m + 0.1m - 3.4m
        assert_eq!(out["goodwill"], json!(1_900_000.0));
        assert_eq!(out["bargain_purchase"], json!(0.0));
    }

    #[test]
    fn acquisition_costs_are_reported_but_not_deducted_from_goodwill() {
        let out = acquire(&deal()).unwrap();
        assert_eq!(out["acquisition_costs"], json!(50_000.0));
        assert_eq!(out["goodwill"], json!(1_900_000.0));
    }

    #[test]
    fn a_cheap_purchase_reports_a_gain_instead_of_negative_goodwill() {
        let mut payload = deal();
        payload["purchase_price"] = json!(1_000_000.0);
        payload["contingent_consideration"] = json!(0.0);
        payload["non_controlling_interest"] = json!(0.0);
        let out = acquire(&payload).unwrap();
        assert_eq!(out["goodwill"], json!(-2_400_000.0));
        assert_eq!(out["bargain_purchase"], json!(2_400_000.0));
    }

    #[test]
    fn withholding_goodwill_removes_the_line_not_the_figure() {
        let mut payload = deal();
        payload["goodwill_recognized"] = json!(false);
        let out = acquire(&payload).unwrap();
        assert_eq!(out["goodwill"], json!(1_900_000.0));
        assert!(out["purchase_price_allocation"]
            .as_array()
            .unwrap()
            .iter()
            .all(|line| line["asset"] != "Goodwill"));
    }
}
