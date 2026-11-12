//! annual-report-service: section accounting, regulatory completeness and the
//! filing deadlines that follow from the fiscal year.
//!
//! The status is decided by one thing only, whether any section is still
//! pending. An audit opinion other than unqualified does not stop a report
//! being ready; it is appended as a note, which is what the service does.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SectionInput {
    #[serde(default)]
    pub name: String,
    #[serde(default = "yes")]
    pub required: bool,
    #[serde(default = "pending")]
    pub status: String,
}

fn yes() -> bool {
    true
}

fn pending() -> String {
    "pending".to_string()
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AnnualReportRequest {
    pub company_id: String,
    pub fiscal_year: String,
    #[serde(default)]
    pub report_type: String,
    #[serde(default)]
    pub jurisdiction: String,
    #[serde(default)]
    pub sections_included: Vec<SectionInput>,
    #[serde(default)]
    pub audit_opinion: String,
    #[serde(default)]
    pub regulatory_filings: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ReportSection {
    pub section_name: String,
    pub required: bool,
    pub completed: bool,
    pub status: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AnnualReport {
    pub company_id: String,
    pub fiscal_year: String,
    pub report_status: String,
    pub sections: Vec<ReportSection>,
    pub completed_sections: i64,
    pub pending_sections: i64,
    pub regulatory_compliance: RegulatoryCompliance,
    pub filing_deadlines: BTreeMap<String, String>,
    pub outstanding_items: Vec<String>,
    pub recommendations: Vec<String>,
}

/// The three filings the service tracks, matched on their exact spelling.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct RegulatoryCompliance {
    pub sec_filing: bool,
    pub stock_exchange: bool,
    pub tax_return: bool,
}

pub fn prepare_annual_report(req: &AnnualReportRequest) -> AnnualReport {
    let mut sections = Vec::with_capacity(req.sections_included.len());
    let mut completed = 0;
    let mut pending = 0;
    for input in &req.sections_included {
        let done = input.status == "completed";
        if done {
            completed += 1;
        } else {
            pending += 1;
        }
        sections.push(ReportSection {
            section_name: input.name.clone(),
            required: input.required,
            completed: done,
            status: input.status.clone(),
        });
    }

    let filed = |wanted: &str| req.regulatory_filings.iter().any(|f| f == wanted);
    let regulatory_compliance = RegulatoryCompliance {
        sec_filing: filed("SEC Filing"),
        stock_exchange: filed("Stock Exchange"),
        tax_return: filed("Tax Return"),
    };

    let outstanding: Vec<String> = sections
        .iter()
        .filter(|s| !s.completed && s.required)
        .map(|s| s.section_name.clone())
        .collect();

    let mut report_status = if pending == 0 {
        "Ready for publication".to_string()
    } else {
        "In preparation".to_string()
    };
    if req.audit_opinion != "unqualified" {
        report_status.push_str(&format!(" ({} opinion)", req.audit_opinion));
    }

    let mut filing_deadlines = BTreeMap::new();
    filing_deadlines.insert(
        "annual_report".to_string(),
        format!("{}-04-30", req.fiscal_year),
    );
    filing_deadlines.insert(
        "sec_filing".to_string(),
        format!("{}-03-31", req.fiscal_year),
    );
    filing_deadlines.insert("agm".to_string(), format!("{}-06-30", req.fiscal_year));

    AnnualReport {
        company_id: req.company_id.clone(),
        fiscal_year: req.fiscal_year.clone(),
        report_status,
        sections,
        completed_sections: completed,
        pending_sections: pending,
        regulatory_compliance,
        filing_deadlines,
        outstanding_items: if outstanding.is_empty() {
            vec!["All required sections completed".to_string()]
        } else {
            outstanding
        },
        recommendations: if pending > 0 {
            vec![
                "Complete all pending sections before filing deadline".to_string(),
                "Obtain board approval for annual report".to_string(),
                "Ensure audit opinion is attached".to_string(),
                "Submit regulatory filings on time".to_string(),
            ]
        } else {
            vec!["Proceed with report publication".to_string()]
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn section(name: &str, status: &str, required: bool) -> SectionInput {
        SectionInput {
            name: name.to_string(),
            required,
            status: status.to_string(),
        }
    }

    fn request() -> AnnualReportRequest {
        AnnualReportRequest {
            company_id: "c1".to_string(),
            fiscal_year: "2026".to_string(),
            report_type: "annual".to_string(),
            jurisdiction: "ZW".to_string(),
            sections_included: vec![
                section("Directors Report", "completed", true),
                section("Financial Statements", "completed", true),
                section("Sustainability", "pending", false),
            ],
            audit_opinion: "unqualified".to_string(),
            regulatory_filings: vec!["SEC Filing".to_string(), "Tax Return".to_string()],
        }
    }

    #[test]
    fn a_pending_optional_section_still_holds_the_report() {
        let report = prepare_annual_report(&request());
        assert_eq!(report.report_status, "In preparation");
        assert_eq!(report.completed_sections, 2);
        assert_eq!(report.pending_sections, 1);
        // it is pending, but not required, so nothing is outstanding
        assert_eq!(
            report.outstanding_items,
            vec!["All required sections completed".to_string()]
        );
        assert!(report.recommendations[0].starts_with("Complete all pending"));
    }

    #[test]
    fn an_unqualified_opinion_needs_no_note() {
        let report = prepare_annual_report(&AnnualReportRequest {
            sections_included: vec![section("Financial Statements", "completed", true)],
            ..request()
        });
        assert_eq!(report.report_status, "Ready for publication");
        assert_eq!(
            report.recommendations,
            vec!["Proceed with report publication".to_string()]
        );
    }

    #[test]
    fn a_qualified_opinion_is_appended_not_blocking() {
        let report = prepare_annual_report(&AnnualReportRequest {
            audit_opinion: "qualified".to_string(),
            sections_included: vec![section("Financial Statements", "completed", true)],
            ..request()
        });
        assert_eq!(
            report.report_status,
            "Ready for publication (qualified opinion)"
        );
    }

    #[test]
    fn outstanding_items_list_only_required_gaps() {
        let report = prepare_annual_report(&AnnualReportRequest {
            sections_included: vec![
                section("Directors Report", "pending", true),
                section("Sustainability", "pending", false),
            ],
            ..request()
        });
        assert_eq!(
            report.outstanding_items,
            vec!["Directors Report".to_string()]
        );
    }

    #[test]
    fn filings_are_matched_on_their_exact_names() {
        let report = prepare_annual_report(&request());
        assert!(report.regulatory_compliance.sec_filing);
        assert!(report.regulatory_compliance.tax_return);
        assert!(!report.regulatory_compliance.stock_exchange);
    }

    #[test]
    fn deadlines_hang_off_the_fiscal_year() {
        let report = prepare_annual_report(&request());
        assert_eq!(report.filing_deadlines["annual_report"], "2026-04-30");
        assert_eq!(report.filing_deadlines["sec_filing"], "2026-03-31");
        assert_eq!(report.filing_deadlines["agm"], "2026-06-30");
    }

    #[test]
    fn an_empty_report_is_in_preparation() {
        let report = prepare_annual_report(&AnnualReportRequest::default());
        assert_eq!(report.pending_sections, 0);
        // nothing pending means ready, even with no sections at all
        assert_eq!(report.report_status, "Ready for publication ( opinion)");
    }
}
