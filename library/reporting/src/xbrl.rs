//! XBRL fact set and filing metadata.
//!
//! This produces the fact list and the filing envelope, not an XBRL instance document - the
//! rendering lives in the bridge crate. Validation here is a five-concept presence check: Revenue
//! from contracts, profit or loss, assets, liabilities and equity must each appear at least once,
//! whatever the taxonomy. Nothing is checked for balance, sign or duplicate concepts.
//!
//! The service minted `id` and `generated_at` from the clock. A pure function has neither, so the
//! caller stamps them; `generated_at` is echoed back when the request supplies it.

use serde_json::{Map, Value};

const REQUIRED_CONCEPTS: [&str; 5] = [
    "RevenueFromContracts",
    "ProfitLoss",
    "Assets",
    "Liabilities",
    "Equity",
];

fn round2(value: f64) -> f64 {
    vimbai_common::numbers::round_dp(value, 2)
}

/// `POST /generate`.
pub fn generate(payload: &Value) -> Result<Value, String> {
    let taxonomy = payload
        .get("taxonomy")
        .and_then(Value::as_str)
        .unwrap_or("ifrs-full")
        .to_string();
    let concepts = payload
        .get("concepts")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let mut facts = Vec::with_capacity(concepts.len());
    let mut seen: Vec<String> = Vec::new();
    for concept in &concepts {
        let name = concept
            .get("concept")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        if !seen.contains(&name) {
            seen.push(name.clone());
        }
        facts.push(serde_json::json!({
            "concept": name,
            "taxonomy_ref": format!("{taxonomy}:{}", concept.get("concept").and_then(Value::as_str).unwrap_or("")),
            "value": round2(concept.get("value").and_then(Value::as_f64).unwrap_or(0.0)),
            "context": concept.get("context_ref").cloned().unwrap_or(Value::Null),
            "unit": concept.get("unit_ref").cloned().unwrap_or(Value::Null),
            "decimals": concept.get("decimals").cloned().unwrap_or(Value::from(0)),
        }));
    }

    let errors: Vec<Value> = REQUIRED_CONCEPTS
        .iter()
        .filter(|required| !seen.iter().any(|name| name == *required))
        .map(|required| Value::String(format!("Missing required concept: {required}")))
        .collect();

    let mut metadata = Map::new();
    metadata.insert(
        "entity_identifier".into(),
        payload
            .get("entity_identifier")
            .cloned()
            .unwrap_or(Value::Null),
    );
    metadata.insert(
        "reporting_period".into(),
        Value::String(format!(
            "FY{}",
            payload.get("fiscal_year").cloned().unwrap_or(Value::Null)
        )),
    );
    if let Some(stamp) = payload.get("generated_at") {
        metadata.insert("generated_at".into(), stamp.clone());
    }
    metadata.insert(
        "schema".into(),
        Value::String(format!("https://xbrl.ifrs.org/taxonomy/{taxonomy}/")),
    );
    metadata.insert("language".into(), Value::String("en".into()));

    let mut out = Map::new();
    for key in ["company_id", "entity_name", "fiscal_year"] {
        out.insert(key.into(), payload.get(key).cloned().unwrap_or(Value::Null));
    }
    out.insert("taxonomy".into(), Value::String(taxonomy));
    out.insert("concept_count".into(), Value::from(concepts.len() as u64));
    out.insert(
        "validation_status".into(),
        Value::String(
            if errors.is_empty() {
                "valid"
            } else {
                "invalid"
            }
            .into(),
        ),
    );
    out.insert("validation_errors".into(), Value::Array(errors));
    out.insert("xbrl_facts".into(), Value::Array(facts));
    out.insert("filing_metadata".into(), Value::Object(metadata));
    Ok(Value::Object(out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn payload() -> Value {
        json!({
            "company_id": "c-1", "fiscal_year": 2026, "entity_name": "Vimbai Ltd",
            "entity_identifier": "ZW-123456", "taxonomy": "ifrs-full",
            "concepts": [
                {"concept": "RevenueFromContracts", "value": 1234567.891,
                 "context_ref": "CY2026", "unit_ref": "ZWL", "decimals": -2},
                {"concept": "ProfitLoss", "value": 234567.5, "context_ref": "CY2026"},
                {"concept": "Assets", "value": 9000000.0, "context_ref": "CY2026"}
            ]
        })
    }

    #[test]
    fn missing_concepts_are_listed_in_the_required_order() {
        let out = generate(&payload()).unwrap();
        assert_eq!(out["validation_status"], json!("invalid"));
        assert_eq!(
            out["validation_errors"],
            json!([
                "Missing required concept: Liabilities",
                "Missing required concept: Equity"
            ])
        );
    }

    #[test]
    fn values_are_rounded_to_two_decimals() {
        let out = generate(&payload()).unwrap();
        assert_eq!(out["xbrl_facts"][0]["value"], json!(1234567.89));
        assert_eq!(
            out["xbrl_facts"][0]["taxonomy_ref"],
            json!("ifrs-full:RevenueFromContracts")
        );
    }

    #[test]
    fn all_five_concepts_validate() {
        let out = generate(&json!({
            "company_id": "c-1", "fiscal_year": 2026, "entity_name": "Vimbai Ltd",
            "entity_identifier": "ZW-1", "taxonomy": "ifrs-full",
            "concepts": [
                {"concept": "RevenueFromContracts", "value": 1.0},
                {"concept": "ProfitLoss", "value": 1.0},
                {"concept": "Assets", "value": 1.0},
                {"concept": "Liabilities", "value": 1.0},
                {"concept": "Equity", "value": 1.0}
            ]
        }))
        .unwrap();
        assert_eq!(out["validation_status"], json!("valid"));
        assert_eq!(out["filing_metadata"]["reporting_period"], json!("FY2026"));
        assert_eq!(
            out["filing_metadata"]["schema"],
            json!("https://xbrl.ifrs.org/taxonomy/ifrs-full/")
        );
    }

    #[test]
    fn a_concept_given_twice_still_counts_once_for_presence() {
        let out = generate(&json!({
            "company_id": "c-1", "fiscal_year": 2026, "entity_name": "e",
            "entity_identifier": "i", "taxonomy": "ifrs-full",
            "concepts": [{"concept": "Assets", "value": 1.0},
                         {"concept": "Assets", "value": 2.0}]
        }))
        .unwrap();
        assert_eq!(out["concept_count"], json!(2));
        assert_eq!(out["validation_errors"].as_array().unwrap().len(), 4);
    }

    #[test]
    fn generated_at_is_echoed_only_when_supplied() {
        let mut stamped = payload();
        stamped["generated_at"] = json!("2026-11-05T09:00:00+00:00");
        let out = generate(&stamped).unwrap();
        assert_eq!(
            out["filing_metadata"]["generated_at"],
            json!("2026-11-05T09:00:00+00:00")
        );
        assert!(generate(&payload()).unwrap()["filing_metadata"]
            .get("generated_at")
            .is_none());
    }
}
