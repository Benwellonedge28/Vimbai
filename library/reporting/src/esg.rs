//! ESG reporting sweep.
//!
//! Environmental metrics are judged in the opposite direction to social and governance ones: a lower
//! number is better, so `value <= target` is on track, while social and governance need
//! `value >= target`. That single direction for social metrics is wrong for incident-style measures -
//! two safety incidents against a target of one counts as on track - and it is left as it was. An environmental metric with no target is always on track, because the default
//! target is infinity.
//!
//! The overall score is simply the share of metrics hitting their target, with all three categories
//! weighted equally by count. `rating_agency_comparison` is a fixed string set - MSCI "A", CDP "B+",
//! Sustainalytics "Low" - whatever the inputs. It is a placeholder that looks like a rating and is
//! not one; a client displaying it should label it as illustrative.

use serde_json::{Map, Value};

fn round2(value: f64) -> f64 {
    vimbai_common::numbers::round_dp(value, 2)
}

fn metric_name(item: &Value) -> String {
    item.get("name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn value_of(item: &Value) -> f64 {
    item.get("value").and_then(Value::as_f64).unwrap_or(0.0)
}

fn category(
    items: &[Value],
    label: &str,
    better_when_lower: bool,
    on_track: &str,
    off_track: &str,
    gap: fn(&str) -> String,
) -> (Vec<Value>, Vec<String>, usize) {
    let mut shaped = Vec::with_capacity(items.len());
    let mut gaps = Vec::new();
    let mut hits = 0usize;

    for item in items {
        let value = value_of(item);
        let target = if better_when_lower {
            item.get("target")
                .and_then(Value::as_f64)
                .unwrap_or(f64::INFINITY)
        } else {
            item.get("target").and_then(Value::as_f64).unwrap_or(0.0)
        };
        let met = if better_when_lower {
            value <= target
        } else {
            value >= target
        };
        if met {
            hits += 1;
        } else {
            gaps.push(gap(&metric_name(item)));
        }
        shaped.push(serde_json::json!({
            "metric_name": metric_name(item),
            "category": label,
            "value": value,
            "unit": item.get("unit").cloned().unwrap_or(Value::String(String::new())),
            "target": item.get("target").cloned().unwrap_or(Value::from(0.0)),
            "status": if met { on_track } else { off_track },
        }));
    }
    (shaped, gaps, hits)
}

/// `POST /report`.
pub fn report(payload: &Value) -> Result<Value, String> {
    let list = |key: &str| -> Vec<Value> {
        payload
            .get(key)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    };

    let (environmental, mut gaps, env_hits) = category(
        &list("environmental_metrics"),
        "Environmental",
        true,
        "on_track",
        "below_target",
        |name| format!("Environmental: {name} target not met"),
    );
    let (social, social_gaps, social_hits) = category(
        &list("social_metrics"),
        "Social",
        false,
        "on_track",
        "below_target",
        |name| format!("Social: {name} target not met"),
    );
    let (governance, gov_gaps, gov_hits) = category(
        &list("governance_metrics"),
        "Governance",
        false,
        "compliant",
        "non_compliant",
        |name| format!("Governance: {name} below threshold"),
    );
    gaps.extend(social_gaps);
    gaps.extend(gov_gaps);

    let total = environmental.len() + social.len() + governance.len();
    let hits = env_hits + social_hits + gov_hits;
    let score = if total == 0 {
        0.0
    } else {
        hits as f64 / total as f64 * 100.0
    };

    let mut out = Map::new();
    for key in ["company_id", "reporting_period", "framework"] {
        out.insert(key.into(), payload.get(key).cloned().unwrap_or(Value::Null));
    }
    out.insert("environmental_metrics".into(), Value::Array(environmental));
    out.insert("social_metrics".into(), Value::Array(social));
    out.insert("governance_metrics".into(), Value::Array(governance));
    out.insert("overall_esg_score".into(), Value::from(round2(score)));
    out.insert(
        "rating_agency_comparison".into(),
        serde_json::json!({
            "msci_rating": "A",
            "cdp_score": "B+",
            "sustainalytics_risk": "Low",
        }),
    );
    out.insert(
        "gaps_identified".into(),
        if gaps.is_empty() {
            serde_json::json!(["All ESG targets on track"])
        } else {
            Value::Array(gaps.into_iter().map(Value::String).collect())
        },
    );
    out.insert(
        "recommendations".into(),
        if total == 0 || out["gaps_identified"] == serde_json::json!(["All ESG targets on track"]) {
            serde_json::json!(["Continue stakeholder engagement", "Enhance ESG disclosures"])
        } else {
            serde_json::json!([
                "Address identified gaps in next reporting period",
                "Set ambitious but achievable targets",
                "Improve data collection processes",
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
            "company_id": "c-1", "reporting_period": "FY2026", "framework": "GRI",
            "environmental_metrics": [
                {"name": "Carbon intensity", "value": 120.0, "unit": "tCO2e/m", "target": 100.0},
                {"name": "Water use", "value": 900.0, "unit": "ML", "target": 1000.0}
            ],
            "social_metrics": [
                {"name": "Safety incidents", "value": 2.0, "target": 1.0},
                {"name": "Training hours", "value": 40.0, "target": 30.0}
            ],
            "governance_metrics": [
                {"name": "Board independence", "value": 0.55, "target": 0.5},
                {"name": "Ethics training", "value": 0.8, "target": 0.95}
            ]
        })
    }

    #[test]
    fn environmental_lower_is_better_while_the_others_are_higher() {
        let out = report(&payload()).unwrap();
        assert_eq!(
            out["environmental_metrics"][0]["status"],
            json!("below_target")
        );
        assert_eq!(out["environmental_metrics"][1]["status"], json!("on_track"));
        // Safety incidents at 2 against a target of 1 count as on track, because every social
        // metric is judged as higher-is-better. Fewer incidents is better, so this reads wrong -
        // it is the service\'s rule and is documented above rather than quietly corrected.
        assert_eq!(out["social_metrics"][0]["status"], json!("on_track"));
        assert_eq!(
            out["governance_metrics"][1]["status"],
            json!("non_compliant")
        );
    }

    #[test]
    fn the_score_is_the_share_of_met_targets() {
        let out = report(&payload()).unwrap();
        // four of six
        assert_eq!(out["overall_esg_score"], json!(66.67));
    }

    #[test]
    fn an_environmental_metric_without_a_target_always_passes() {
        let out = report(&json!({
            "company_id": "c-1", "reporting_period": "FY2026", "framework": "GRI",
            "environmental_metrics": [{"name": "Emissions", "value": 999.0}]
        }))
        .unwrap();
        assert_eq!(out["environmental_metrics"][0]["status"], json!("on_track"));
        assert_eq!(out["overall_esg_score"], json!(100.0));
    }

    #[test]
    fn the_rating_comparison_is_a_fixed_placeholder() {
        let out = report(&json!({"company_id": "c-9"})).unwrap();
        assert_eq!(out["rating_agency_comparison"]["msci_rating"], json!("A"));
        assert_eq!(out["overall_esg_score"], json!(0.0));
    }

    #[test]
    fn no_gaps_switches_to_steady_state_advice() {
        let out = report(&json!({
            "company_id": "c-1", "reporting_period": "FY2026", "framework": "GRI",
            "social_metrics": [{"name": "Training", "value": 10.0, "target": 5.0}]
        }))
        .unwrap();
        assert_eq!(out["gaps_identified"], json!(["All ESG targets on track"]));
        assert_eq!(
            out["recommendations"],
            json!(["Continue stakeholder engagement", "Enhance ESG disclosures"])
        );
    }
}
