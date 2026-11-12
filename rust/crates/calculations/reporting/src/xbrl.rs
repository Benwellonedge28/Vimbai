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

pub const REQUIRED_CONCEPTS: [&str; 5] = [
    "RevenueFromContracts",
    "ProfitLoss",
    "Assets",
    "Liabilities",
    "Equity",
];

fn round2(value: f64) -> f64 {
    vimbai_common::numbers::round_dp(value, 2)
}

/// One XBRL fact. Both filings this workspace produces - the statement filing and the tax filing -
/// emit exactly these six keys, so they are built here and nowhere else. `value` is money and is
/// rounded to two decimals; a fact list that carries a different rounding is a different fact set.
pub fn fact(
    taxonomy: &str,
    concept: &str,
    value: f64,
    context: Value,
    unit: Value,
    decimals: Value,
) -> Value {
    serde_json::json!({
        "concept": concept,
        "taxonomy_ref": format!("{taxonomy}:{concept}"),
        "value": round2(value),
        "context": context,
        "unit": unit,
        "decimals": decimals,
    })
}

/// The concepts a filing is rejected without, that `names` does not contain.
///
/// A fact list missing any of the five is `invalid` and the error names the concept - the same
/// sentence whichever filing asked.
pub fn missing_concepts<'a>(names: impl Iterator<Item = &'a str>) -> Vec<String> {
    let seen: Vec<&str> = names.collect();
    REQUIRED_CONCEPTS
        .iter()
        .filter(|required| !seen.contains(*required))
        .map(|required| format!("Missing required concept: {required}"))
        .collect()
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

    let names: Vec<&str> = concepts
        .iter()
        .map(|concept| concept.get("concept").and_then(Value::as_str).unwrap_or(""))
        .collect();
    let facts: Vec<Value> = concepts
        .iter()
        .zip(&names)
        .map(|(concept, name)| {
            fact(
                &taxonomy,
                name,
                concept.get("value").and_then(Value::as_f64).unwrap_or(0.0),
                concept.get("context_ref").cloned().unwrap_or(Value::Null),
                concept.get("unit_ref").cloned().unwrap_or(Value::Null),
                concept.get("decimals").cloned().unwrap_or(Value::from(0)),
            )
        })
        .collect();

    let errors: Vec<Value> = missing_concepts(names.into_iter())
        .into_iter()
        .map(Value::String)
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
