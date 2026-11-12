//! Corporate governance compliance sweep.
//!
//! Four metrics decide the score and two of them can carry the same area name, so
//! `non_compliant_areas` may contain "Board Composition" twice - that duplication is the service's,
//! and a screen listing areas would show it twice.
//!
//! The audit committee check is always compliant: every member being independent is asserted rather
//! than tested. No committee data is consulted for it.

use serde_json::{Map, Value};

fn round2(value: f64) -> f64 {
    vimbai_common::numbers::round_dp(value, 2)
}

fn metric(area: &str, requirement: &str, compliant: bool, gap: &str) -> Value {
    serde_json::json!({
        "area": area,
        "requirement": requirement,
        "compliant": compliant,
        "gap": gap,
    })
}

/// `POST /assess`.
pub fn assess(payload: &Value) -> Result<Value, String> {
    let composition = payload.get("board_composition");
    let number = |key: &str| -> f64 {
        composition
            .and_then(|c| c.get(key))
            .and_then(Value::as_f64)
            .unwrap_or(0.0)
    };
    let total_directors = number("total_directors");
    let independent_directors = number("independent_directors");
    let female_directors = number("female_directors");
    let average_tenure = composition
        .and_then(|c| c.get("avg_tenure"))
        .cloned()
        .unwrap_or(Value::from(5));

    let independence_ratio = if total_directors == 0.0 {
        0.0
    } else {
        independent_directors / total_directors
    };
    let gender_diversity = if total_directors == 0.0 {
        0.0
    } else {
        female_directors / total_directors
    };
    let has_framework = payload
        .get("risk_management")
        .and_then(|r| r.get("framework"))
        .map(|f| !matches!(f, Value::Null) && f.as_str() != Some("") && f.as_bool() != Some(false))
        .unwrap_or(false);

    let metrics = vec![
        metric(
            "Board Composition",
            "At least 30% independent directors",
            independence_ratio >= 0.3,
            if independence_ratio < 0.3 {
                "Independent director threshold not met"
            } else {
                "Compliant"
            },
        ),
        metric(
            "Board Composition",
            "Gender diversity target",
            gender_diversity >= 0.3,
            if gender_diversity < 0.3 {
                "Gender diversity target not met"
            } else {
                "Compliant"
            },
        ),
        metric("Audit Committee", "All independent members", true, ""),
        metric(
            "Risk Management",
            "Documented risk framework",
            has_framework,
            if has_framework {
                "Compliant"
            } else {
                "Risk framework needs strengthening"
            },
        ),
    ];

    let compliant_count = metrics
        .iter()
        .filter(|m| m["compliant"] == Value::Bool(true))
        .count();
    let score = if metrics.is_empty() {
        0.0
    } else {
        compliant_count as f64 / metrics.len() as f64 * 100.0
    };
    let non_compliant: Vec<Value> = metrics
        .iter()
        .filter(|m| m["compliant"] == Value::Bool(false))
        .map(|m| m["area"].clone())
        .collect();

    let committee_effectiveness: Vec<Value> = payload
        .get("committees")
        .and_then(Value::as_array)
        .map(|rows| {
            rows.iter()
                .map(|committee| {
                    let meetings = committee
                        .get("meetings_held")
                        .and_then(Value::as_f64)
                        .unwrap_or(0.0);
                    serde_json::json!({
                        "name": committee.get("name").cloned().unwrap_or(Value::Null),
                        "members": committee.get("members").cloned().unwrap_or(Value::from(0)),
                        "meetings": meetings,
                        "effective": meetings >= 4.0,
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    let mut out = Map::new();
    for key in ["company_id", "jurisdiction", "governance_code"] {
        out.insert(key.into(), payload.get(key).cloned().unwrap_or(Value::Null));
    }
    out.insert(
        "overall_compliance_score".into(),
        Value::from(round2(score)),
    );
    out.insert(
        "board_metrics".into(),
        serde_json::json!({
            "total_directors": total_directors,
            "independent_directors": independent_directors,
            "independence_ratio": round2(independence_ratio),
            "gender_diversity": round2(gender_diversity),
            "average_tenure_years": average_tenure,
        }),
    );
    out.insert(
        "committee_effectiveness".into(),
        Value::Array(committee_effectiveness),
    );
    out.insert("compliance_metrics".into(), Value::Array(metrics));
    out.insert(
        "non_compliant_areas".into(),
        if non_compliant.is_empty() {
            serde_json::json!(["All governance requirements met"])
        } else {
            Value::Array(non_compliant.clone())
        },
    );
    out.insert(
        "recommendations".into(),
        if non_compliant.is_empty() {
            serde_json::json!(["Continue monitoring governance compliance"])
        } else {
            serde_json::json!([
                "Recruit additional independent directors",
                "Increase gender diversity on the board",
                "Ensure all committees meet minimum meeting requirements",
                "Review and update governance policies annually",
            ])
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
            "company_id": "c-1", "jurisdiction": "ZW", "governance_code": "King IV",
            "board_composition": {"total_directors": 8, "independent_directors": 2,
                                  "female_directors": 3, "avg_tenure": 4.5},
            "committees": [{"name": "Audit", "members": 4, "meetings_held": 5},
                           {"name": "Risk", "members": 3, "meetings_held": 2}],
            "policies": ["Code of conduct"],
            "risk_management": {"framework": "ISO 31000"}
        })
    }

    #[test]
    fn one_gap_scores_three_of_four() {
        let out = assess(&payload()).unwrap();
        assert_eq!(out["overall_compliance_score"], json!(75.0));
        assert_eq!(out["non_compliant_areas"], json!(["Board Composition"]));
    }

    #[test]
    fn the_duplicated_area_name_is_kept() {
        let out = assess(&json!({
            "company_id": "c-1", "jurisdiction": "ZW", "governance_code": "King IV",
            "board_composition": {"total_directors": 10, "independent_directors": 1,
                                  "female_directors": 0}
        }))
        .unwrap();
        assert_eq!(
            out["non_compliant_areas"],
            json!(["Board Composition", "Board Composition", "Risk Management"])
        );
        assert_eq!(out["overall_compliance_score"], json!(25.0));
    }

    #[test]
    fn the_audit_committee_is_always_compliant() {
        let out = assess(&json!({
            "company_id": "c-1", "jurisdiction": "ZW", "governance_code": "King IV",
            "board_composition": {"total_directors": 0}
        }))
        .unwrap();
        let audit = out["compliance_metrics"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["area"] == "Audit Committee")
            .cloned()
            .unwrap();
        assert_eq!(audit["compliant"], json!(true));
    }

    #[test]
    fn a_missing_tenure_defaults_to_five_years() {
        let out = assess(&json!({
            "company_id": "c-1", "jurisdiction": "ZW", "governance_code": "King IV",
            "board_composition": {"total_directors": 4}
        }))
        .unwrap();
        assert_eq!(out["board_metrics"]["average_tenure_years"], json!(5));
    }

    #[test]
    fn committees_are_effective_from_four_meetings() {
        let out = assess(&payload()).unwrap();
        assert_eq!(out["committee_effectiveness"][0]["effective"], json!(true));
        assert_eq!(out["committee_effectiveness"][1]["effective"], json!(false));
    }
}
