//! regulatory-reporting-service: the prudential returns a bank files, their
//! filing reference and the checks that decide whether a return may be sent.
//!
//! PYTHON QUIRK: only four of the seven report types carry a threshold check.
//! The capital adequacy, related party, AML and FX exposure returns pass on data
//! completeness alone, which is worth knowing before trusting a green status.
use serde::{Deserialize, Serialize};
use serde_json::Map;
use vimbai_common::py_round2;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ReportRequest {
    pub company_id: String,
    /// `prudential`, `liquidity`, `capital_adequacy`, `large_exposures`,
    /// `related_party`, `aml` or `fx_exposure`.
    #[serde(default)]
    pub report_type: String,
    #[serde(default)]
    pub period: String,
    #[serde(default = "jurisdiction")]
    pub jurisdiction: String,
    #[serde(default)]
    pub data: Map<String, serde_json::Value>,
}

fn jurisdiction() -> String {
    "ZW".to_string()
}

impl ReportRequest {
    fn figure(&self, key: &str) -> f64 {
        self.data.get(key).and_then(|v| v.as_f64()).unwrap_or(0.0)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ValidationCheck {
    pub check: String,
    pub value: serde_json::Value,
    pub pass: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReportResult {
    pub id: String,
    pub company_id: String,
    pub report_type: String,
    pub period: String,
    pub jurisdiction: String,
    pub status: String,
    pub filing_reference: String,
    pub summary: Map<String, serde_json::Value>,
    pub validation_checks: Vec<ValidationCheck>,
    pub submission_deadline: String,
}

/// `/generate`. The id and the deadline are inputs: the service draws a UUID and
/// counts thirty days from the wall clock, neither of which is reproducible.
pub fn generate_report(
    req: &ReportRequest,
    report_id: &str,
    submission_deadline: &str,
) -> ReportResult {
    let filing_reference = format!(
        "REG-{}-{}-{}",
        req.jurisdiction,
        req.report_type.to_uppercase(),
        req.period.replace('-', "")
    );

    let mut validation = Vec::new();
    let mut all_pass = true;
    let mut check = |name: String, value: serde_json::Value, pass: bool| {
        validation.push(ValidationCheck {
            check: name,
            value,
            pass,
        });
        if !pass {
            all_pass = false;
        }
    };

    match req.report_type.as_str() {
        "prudential" => {
            let car = req.figure("capital_ratio");
            check(
                "Capital Adequacy Ratio >= 12%".to_string(),
                car.into(),
                car >= 12.0,
            );
        }
        "liquidity" => {
            let lcr = req.figure("liquidity_ratio");
            check(
                "Liquidity Coverage Ratio >= 100%".to_string(),
                lcr.into(),
                lcr >= 100.0,
            );
        }
        "large_exposures" => {
            let max_exp = req.figure("largest_exposure_pct");
            check(
                "Single exposure <= 25% of capital".to_string(),
                max_exp.into(),
                max_exp <= 25.0,
            );
        }
        // every other return is checked for completeness only
        _ => {}
    }

    let count = req.data.len();
    check("Data completeness".to_string(), count.into(), count > 0);

    let summary: Map<String, serde_json::Value> = req
        .data
        .iter()
        .map(|(k, v)| {
            let rounded = match v.as_f64() {
                Some(f) => py_round2(f).into(),
                None => v.clone(),
            };
            (k.clone(), rounded)
        })
        .collect();

    ReportResult {
        id: report_id.to_string(),
        company_id: req.company_id.clone(),
        report_type: req.report_type.clone(),
        period: req.period.clone(),
        jurisdiction: req.jurisdiction.clone(),
        status: if all_pass {
            "ready_for_submission".to_string()
        } else {
            "validation_failed".to_string()
        },
        filing_reference,
        summary,
        validation_checks: validation,
        submission_deadline: submission_deadline.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn request(report_type: &str, data: serde_json::Value) -> ReportRequest {
        serde_json::from_value(json!({
            "company_id": "c1",
            "report_type": report_type,
            "period": "2026-09",
            "jurisdiction": "ZW",
            "data": data
        }))
        .unwrap()
    }

    #[test]
    fn the_filing_reference_names_the_return() {
        let result = generate_report(
            &request("prudential", json!({"capital_ratio": 14.5})),
            "id",
            "2026-12-02",
        );
        assert_eq!(result.filing_reference, "REG-ZW-PRUDENTIAL-202609");
    }

    #[test]
    fn a_thin_capital_ratio_fails_the_return() {
        let result = generate_report(
            &request("prudential", json!({"capital_ratio": 11.9})),
            "id",
            "2026-12-02",
        );
        assert_eq!(result.status, "validation_failed");
        assert_eq!(result.validation_checks.len(), 2);
        assert!(!result.validation_checks[0].pass);
        assert_eq!(result.validation_checks[0].value, 11.9);
    }

    #[test]
    fn an_exposure_of_exactly_twenty_five_percent_passes() {
        let result = generate_report(
            &request("large_exposures", json!({"largest_exposure_pct": 25})),
            "id",
            "2026-12-02",
        );
        assert_eq!(result.status, "ready_for_submission");
        assert!(result.validation_checks[0].pass);
    }

    #[test]
    fn four_return_types_check_completeness_only() {
        for kind in ["capital_adequacy", "related_party", "aml", "fx_exposure"] {
            let result = generate_report(&request(kind, json!({"anything": 1})), "id", "d");
            assert_eq!(result.validation_checks.len(), 1);
            assert_eq!(result.validation_checks[0].check, "Data completeness");
            assert_eq!(result.status, "ready_for_submission");
        }
    }

    #[test]
    fn an_empty_return_fails_completeness() {
        let result = generate_report(&request("aml", json!({})), "id", "d");
        assert_eq!(result.status, "validation_failed");
        assert_eq!(result.validation_checks[0].value, 0);
    }

    #[test]
    fn the_summary_rounds_and_keeps_the_order() {
        let result = generate_report(
            &request(
                "prudential",
                json!({"tier_one": 1234.567, "capital_ratio": 14}),
            ),
            "id",
            "d",
        );
        let keys: Vec<&String> = result.summary.keys().collect();
        assert_eq!(keys, vec!["tier_one", "capital_ratio"]);
        assert_eq!(result.summary["tier_one"], 1234.57);
    }
}
