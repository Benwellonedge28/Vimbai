//! annual-report-service `/prepare`: publication readiness, section
//! completion, regulatory filings and filing deadlines.
use serde::{Deserialize, Serialize};

/// One entry of `sections_included`, which the Python service reads as a
/// plain dict with `name`, `required` and `status` keys.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SectionInput {
    pub name: Option<String>,
    pub required: Option<bool>,
    pub status: Option<String>,
}

impl Default for SectionInput {
    fn default() -> Self {
        Self {
            name: None,
            required: Some(true),
            status: None,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AnnualReportInput {
    pub company_id: String,
    pub fiscal_year: String,
    pub report_type: String,
    pub jurisdiction: String,
    pub sections_included: Vec<SectionInput>,
    pub audit_opinion: String,
    pub regulatory_filings: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReportSection {
    pub section_name: String,
    pub required: bool,
    pub completed: bool,
    pub status: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RegulatoryCompliance {
    pub sec_filing: bool,
    pub stock_exchange: bool,
    pub tax_return: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FilingDeadlines {
    pub annual_report: String,
    pub sec_filing: String,
    pub agm: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AnnualReport {
    pub company_id: String,
    pub fiscal_year: String,
    pub report_status: String,
    pub sections: Vec<ReportSection>,
    pub completed_sections: usize,
    pub pending_sections: usize,
    pub regulatory_compliance: RegulatoryCompliance,
    pub filing_deadlines: FilingDeadlines,
    pub outstanding_items: Vec<String>,
    pub recommendations: Vec<String>,
}

/// PYTHON QUIRKS, all preserved:
/// - any status other than the exact string "completed" counts as pending,
///   including a missing status;
/// - `pending` counts every unfinished section while `outstanding_items`
///   lists only unfinished *required* ones;
/// - recommendations are driven by `pending`, so a pending optional
///   section still says to complete all pending sections;
/// - the audit suffix is appended to the readiness text, giving
///   "In preparation (qualified opinion)".
pub fn prepare_annual_report(req: &AnnualReportInput) -> AnnualReport {
    let mut sections = Vec::new();
    let mut completed = 0usize;
    let mut pending = 0usize;

    for section in &req.sections_included {
        let status = section
            .status
            .clone()
            .unwrap_or_else(|| "pending".to_string());
        let is_completed = status == "completed";
        if is_completed {
            completed += 1;
        } else {
            pending += 1;
        }
        sections.push(ReportSection {
            section_name: section.name.clone().unwrap_or_default(),
            required: section.required.unwrap_or(true),
            completed: is_completed,
            status,
        });
    }

    let regulatory_compliance = RegulatoryCompliance {
        sec_filing: req.regulatory_filings.iter().any(|f| f == "SEC Filing"),
        stock_exchange: req.regulatory_filings.iter().any(|f| f == "Stock Exchange"),
        tax_return: req.regulatory_filings.iter().any(|f| f == "Tax Return"),
    };

    let mut outstanding: Vec<String> = sections
        .iter()
        .filter(|s| !s.completed && s.required)
        .map(|s| s.section_name.clone())
        .collect();
    if outstanding.is_empty() {
        outstanding.push("All required sections completed".to_string());
    }

    let mut report_status = if pending == 0 {
        "Ready for publication".to_string()
    } else {
        "In preparation".to_string()
    };
    if req.audit_opinion != "unqualified" {
        report_status.push_str(&format!(" ({} opinion)", req.audit_opinion));
    }

    let recommendations = if pending > 0 {
        vec![
            "Complete all pending sections before filing deadline".to_string(),
            "Obtain board approval for annual report".to_string(),
            "Ensure audit opinion is attached".to_string(),
            "Submit regulatory filings on time".to_string(),
        ]
    } else {
        vec!["Proceed with report publication".to_string()]
    };

    AnnualReport {
        company_id: req.company_id.clone(),
        fiscal_year: req.fiscal_year.clone(),
        report_status,
        sections,
        completed_sections: completed,
        pending_sections: pending,
        regulatory_compliance,
        filing_deadlines: FilingDeadlines {
            annual_report: format!("{}-04-30", req.fiscal_year),
            sec_filing: format!("{}-03-31", req.fiscal_year),
            agm: format!("{}-06-30", req.fiscal_year),
        },
        outstanding_items: outstanding,
        recommendations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn section(name: &str, status: &str, required: bool) -> SectionInput {
        SectionInput {
            name: Some(name.into()),
            required: Some(required),
            status: Some(status.into()),
        }
    }

    fn input() -> AnnualReportInput {
        AnnualReportInput {
            company_id: "c1".into(),
            fiscal_year: "2026".into(),
            report_type: "statutory".into(),
            jurisdiction: "ZW".into(),
            sections_included: vec![
                section("Directors Report", "completed", true),
                section("Financial Statements", "in_progress", true),
                section("Sustainability", "pending", false),
            ],
            audit_opinion: "unqualified".into(),
            regulatory_filings: vec!["SEC Filing".into(), "Tax Return".into(), "Other".into()],
        }
    }

    #[test]
    fn sections_and_counts() {
        let r = prepare_annual_report(&input());
        assert_eq!(r.completed_sections, 1);
        assert_eq!(r.pending_sections, 2);
        assert!(r.sections[0].completed);
        assert_eq!(r.sections[1].status, "in_progress");
        assert!(!r.sections[2].required);
    }

    #[test]
    fn only_required_sections_are_outstanding() {
        let r = prepare_annual_report(&input());
        assert_eq!(
            r.outstanding_items,
            vec!["Financial Statements".to_string()]
        );
    }

    #[test]
    fn optional_pending_section_still_triggers_full_advice() {
        let r = prepare_annual_report(&AnnualReportInput {
            sections_included: vec![
                section("Directors Report", "completed", true),
                section("Sustainability", "pending", false),
            ],
            ..input()
        });
        // nothing required is missing, yet the advice list is the full one
        assert_eq!(
            r.outstanding_items,
            vec!["All required sections completed".to_string()]
        );
        assert_eq!(r.pending_sections, 1);
        assert_eq!(r.report_status, "In preparation");
        assert_eq!(r.recommendations.len(), 4);
    }

    #[test]
    fn ready_status_with_non_unqualified_opinion_suffix() {
        let r = prepare_annual_report(&AnnualReportInput {
            sections_included: vec![section("Directors Report", "completed", true)],
            audit_opinion: "qualified".into(),
            ..input()
        });
        assert_eq!(r.report_status, "Ready for publication (qualified opinion)");
        let ready = prepare_annual_report(&AnnualReportInput {
            sections_included: vec![section("Directors Report", "completed", true)],
            audit_opinion: "disclaimer".into(),
            ..input()
        });
        assert_eq!(
            ready.report_status,
            "Ready for publication (disclaimer opinion)"
        );
        assert_eq!(
            ready.recommendations,
            vec!["Proceed with report publication".to_string()]
        );
    }

    #[test]
    fn regulatory_flags_and_deadlines() {
        let r = prepare_annual_report(&input());
        assert!(r.regulatory_compliance.sec_filing);
        assert!(r.regulatory_compliance.tax_return);
        assert!(!r.regulatory_compliance.stock_exchange);
        assert_eq!(r.filing_deadlines.annual_report, "2026-04-30");
        assert_eq!(r.filing_deadlines.sec_filing, "2026-03-31");
        assert_eq!(r.filing_deadlines.agm, "2026-06-30");
    }

    #[test]
    fn missing_status_counts_as_pending() {
        let r = prepare_annual_report(&AnnualReportInput {
            sections_included: vec![SectionInput {
                name: Some("Cash Flow".into()),
                required: None,
                status: None,
            }],
            ..input()
        });
        assert_eq!(r.sections[0].status, "pending");
        assert!(r.sections[0].required);
        assert_eq!(r.pending_sections, 1);
    }
}
