//! Statutory filing assessment, as the service performed it.
//!
//! A filing is outstanding until it is marked submitted, and only an outstanding filing contributes
//! its late penalty to the exposure figure - a submitted filing's recorded penalty is ignored rather
//! than refunded. When nothing is outstanding the service lists the sentence "All required filings
//! submitted" in place of an empty list, and swaps its recommendations for a single line; both are
//! kept because a client screen renders them directly.

use serde_json::{Map, Value};

fn number(value: Option<&Value>) -> f64 {
    match value {
        Some(Value::Number(n)) => n.as_f64().unwrap_or(0.0),
        Some(Value::String(s)) => s.trim().parse().unwrap_or(0.0),
        _ => 0.0,
    }
}

fn string_of(value: Option<&Value>) -> Value {
    match value {
        Some(Value::String(s)) => Value::String(s.clone()),
        Some(other) => Value::String(other.to_string()),
        None => Value::String(String::new()),
    }
}

/// `POST /assess` of the statutory filing service.
pub fn assess(payload: &Value) -> Result<Value, String> {
    let filings = payload
        .get("filings_required")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let mut requirements = Vec::with_capacity(filings.len());
    let mut deadlines = Vec::new();
    let mut outstanding = Vec::new();
    let mut exposure = 0.0;

    for filing in &filings {
        let submitted = filing
            .get("submitted")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let penalty = if submitted {
            0.0
        } else {
            number(filing.get("late_penalty"))
        };
        exposure += penalty;

        if !submitted {
            outstanding.push(Value::String(format!(
                "{} - {}",
                filing
                    .get("type")
                    .map(|v| v.to_string().trim_matches('"').to_string())
                    .unwrap_or_default(),
                filing
                    .get("jurisdiction")
                    .map(|v| v.to_string().trim_matches('"').to_string())
                    .unwrap_or_default()
            )));
            deadlines.push(serde_json::json!({
                "type": filing.get("type").cloned().unwrap_or(Value::Null),
                "deadline": filing.get("deadline").cloned().unwrap_or(Value::String(String::new())),
            }));
        }

        requirements.push(serde_json::json!({
            "filing_type": string_of(filing.get("type")),
            "jurisdiction": string_of(filing.get("jurisdiction")),
            "deadline": string_of(filing.get("deadline")),
            "status": if submitted { "submitted" } else { "pending" },
            "submitted": submitted,
            "late_filing_penalty": penalty,
        }));
    }

    let nothing_outstanding = outstanding.is_empty();
    let mut out = Map::new();
    out.insert(
        "company_id".into(),
        payload.get("company_id").cloned().unwrap_or(Value::Null),
    );
    out.insert(
        "fiscal_year".into(),
        payload.get("fiscal_year").cloned().unwrap_or(Value::Null),
    );
    out.insert("filing_requirements".into(), Value::Array(requirements));
    out.insert("upcoming_deadlines".into(), Value::Array(deadlines));
    out.insert(
        "compliance_status".into(),
        Value::String(
            if nothing_outstanding {
                "compliant"
            } else {
                "pending_filings"
            }
            .into(),
        ),
    );
    out.insert(
        "outstanding_filings".into(),
        if nothing_outstanding {
            serde_json::json!(["All required filings submitted"])
        } else {
            Value::Array(outstanding)
        },
    );
    out.insert(
        "late_filing_exposure".into(),
        Value::from(vimbai_common::numbers::round_dp(exposure, 2)),
    );
    out.insert(
        "recommendations".into(),
        if nothing_outstanding {
            serde_json::json!(["Maintain compliance tracking"])
        } else {
            serde_json::json!([
                "Submit all outstanding filings before deadlines",
                "Review filing requirements for each jurisdiction",
                "Set up filing calendar reminders",
            ])
        },
    );
    Ok(Value::Object(out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_submitted_filing_does_not_add_its_recorded_penalty() {
        let out = assess(&json!({
            "company_id": "c-1", "fiscal_year": "2026",
            "filings_required": [
                {"type": "VAT", "jurisdiction": "ZW", "deadline": "2026-04-25", "submitted": true, "late_penalty": 99.0}
            ]
        }))
        .unwrap();
        assert_eq!(out["late_filing_exposure"], json!(0.0));
        assert_eq!(out["compliance_status"], json!("compliant"));
        assert_eq!(
            out["filing_requirements"][0]["late_filing_penalty"],
            json!(0.0)
        );
    }

    #[test]
    fn nothing_outstanding_reads_as_a_sentence_not_an_empty_list() {
        let out =
            assess(&json!({"company_id": "c-1", "fiscal_year": "2026", "filings_required": []}))
                .unwrap();
        assert_eq!(
            out["outstanding_filings"],
            json!(["All required filings submitted"])
        );
        assert_eq!(
            out["recommendations"],
            json!(["Maintain compliance tracking"])
        );
    }

    #[test]
    fn an_outstanding_filing_names_itself_and_its_jurisdiction() {
        let out = assess(&json!({
            "company_id": "c-1", "fiscal_year": "2026",
            "filings_required": [{"type": "PAYE", "jurisdiction": "ZW", "deadline": "2026-05-10", "submitted": false, "late_penalty": 120.0}]
        }))
        .unwrap();
        assert_eq!(out["outstanding_filings"], json!(["PAYE - ZW"]));
        assert_eq!(out["late_filing_exposure"], json!(120.0));
        assert_eq!(out["upcoming_deadlines"][0]["type"], json!("PAYE"));
    }

    #[test]
    fn a_filing_without_a_submitted_flag_is_pending() {
        let out = assess(&json!({
            "company_id": "c-1", "fiscal_year": "2026",
            "filings_required": [{"type": "CIT", "jurisdiction": "ZW"}]
        }))
        .unwrap();
        assert_eq!(out["filing_requirements"][0]["status"], json!("pending"));
        assert_eq!(out["company_id"], json!("c-1"));
    }
}
