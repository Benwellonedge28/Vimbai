#![forbid(unsafe_code)]
//! treasury-compliance-service: the regulation checks a treasury runs, their verdicts, and the report over them.
//!
//! Shapes kept from the service:
//!
//! * the first read of a company's checks seeds the six default checks and keeps them, so a second read finds
//!   them rather than seeding again;
//! * the report never seeds, because a report that has never had checks run should say so instead of quietly
//!   creating them, and it says `Run compliance checks first` with a rate of one;
//! * a rate is compliant over total, and over nothing it is one rather than a division by zero;
//! * a finding is a check that is a warning or not compliant, and the recommendations are the remediation notes
//!   of those findings, so an empty note contributes nothing;
//! * a check in another Book is a miss, not a refusal that admits it exists.

use serde_json::{json, Value};

use vimbai_store::query::query_page;
use vimbai_store::{BookScope, Graph, Records, StoreError, TREASURY_COMPLIANCE_CHECK};

use crate::helpers::{field, or_default, remember, text};

/// The verdicts a check can carry.
pub const STATUSES: [&str; 4] = ["compliant", "warning", "non_compliant", "pending_review"];

/// The checks every company starts with, seeded on first read.
pub const DEFAULT_CHECKS: [(&str, &str, &str); 6] = [
    (
        "Counterparty Limit Compliance",
        "Basel III",
        "Ensure counterparty exposure is within regulatory limits",
    ),
    (
        "Liquidity Coverage Ratio",
        "Basel III LCR",
        "Maintain LCR above 100%",
    ),
    (
        "FX Exposure Limits",
        "Internal Policy",
        "Verify foreign exchange exposure within approved limits",
    ),
    (
        "Investment Guidelines",
        "Board Policy",
        "Ensure investments comply with board-approved guidelines",
    ),
    (
        "Segregation of Duties",
        "SOX",
        "Verify treasury duties are properly segregated",
    ),
    (
        "Reporting Timeliness",
        "Regulatory",
        "Ensure regulatory reports submitted on time",
    ),
];

/// `GET /checks/{company_id}`: the checks, seeding them the first time this company is read.
pub fn list_checks<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    company_id: &str,
) -> Result<Value, StoreError> {
    let mut checks = mine(graph, scope, company_id)?;
    if checks.is_empty() {
        for (name, regulation, details) in DEFAULT_CHECKS {
            let stored = remember(
                graph,
                scope,
                TREASURY_COMPLIANCE_CHECK,
                json!({
                    "company_id": company_id,
                    "check_name": name,
                    "regulation": regulation,
                    "details": details,
                    "status": "pending_review",
                    "remediation": "",
                    "checked_at": "",
                }),
            )?;
            checks.push(stored);
        }
    }

    let total = checks.len();
    let compliant = checks
        .iter()
        .filter(|check| field(check, "status") == "compliant")
        .count();

    Ok(json!({
        "company_id": company_id,
        "checks": checks,
        "total": total,
        "compliant": compliant,
        // over nothing a rate is one, which is what the service reported
        "compliance_rate": compliant as f64 / total.max(1) as f64,
    }))
}

/// `PUT /checks/{check_id}/status`: record a verdict, with an optional note on what fixes it.
pub fn update_status<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let check_id = text(payload, "check_id");
    let status = or_default(payload, "status", "pending_review");
    if !STATUSES.contains(&status.as_str()) {
        return Err(StoreError::Invalid(format!(
            "Invalid status. Must be one of {STATUSES:?}"
        )));
    }

    let Some(_check) = mine(graph, scope, field(payload, "company_id"))?
        .into_iter()
        .find(|row| field(row, "id") == check_id)
    else {
        // a check the caller cannot see is missing, whether or not someone else holds it
        return Err(StoreError::NotFound);
    };

    Records::new(graph, TREASURY_COMPLIANCE_CHECK).update(
        scope,
        &check_id,
        &json!({
            "status": status,
            "remediation": text(payload, "remediation"),
            "checked_at": text(payload, "checked_at"),
        }),
    )?;

    Ok(json!({ "check_id": check_id, "status": status }))
}

/// `GET /report/{company_id}`: the report over the checks that exist, never seeding any.
pub fn report<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    company_id: &str,
) -> Result<Value, StoreError> {
    let checks = mine(graph, scope, company_id)?;
    if checks.is_empty() {
        return Ok(json!({
            "company_id": company_id,
            "compliance_rate": 1.0,
            "findings": [],
            "recommendations": ["Run compliance checks first"],
        }));
    }

    // the rate is over every check, including the ones nobody has reviewed yet
    let total = checks.len();
    let compliant = checks
        .iter()
        .filter(|check| field(check, "status") == "compliant")
        .count();
    let findings: Vec<Value> = checks
        .into_iter()
        .filter(|check| {
            let status = field(check, "status");
            status == "warning" || status == "non_compliant"
        })
        .collect();
    let recommendations: Vec<String> = findings
        .iter()
        .map(|finding| field(finding, "remediation").to_string())
        .filter(|note| !note.is_empty())
        .collect();

    Ok(json!({
        "company_id": company_id,
        "compliance_rate": compliant as f64 / total as f64,
        "total_findings": findings.len(),
        "findings": findings,
        "recommendations": recommendations,
    }))
}

/// The caller's checks for a company, in the order they were written.
fn mine<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    company_id: &str,
) -> Result<Vec<Value>, StoreError> {
    let mut rows: Vec<Value> = Records::new(graph, TREASURY_COMPLIANCE_CHECK)
        .list(scope, query_page(&json!({}), 4000, 4000))?
        .into_iter()
        .filter(|row| field(row, "company_id") == company_id)
        .collect();
    rows.sort_by(|one, two| field(one, "created_at").cmp(field(two, "created_at")));
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::FakeGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn check(id: &str, status: &str, remediation: &str) -> Value {
        json!({"id": id, "company_id": "c-1", "status": status, "remediation": remediation,
               "created_at": "2026-03-01T00:00:00"})
    }

    #[test]
    fn the_first_read_seeds_the_six_default_checks_and_keeps_them() {
        let seeded: Vec<Vec<Value>> = (0..6)
            .map(|n| vec![json!({ "id": format!("k-{n}") })])
            .collect();
        let mut scripts = vec![vec![]];
        scripts.extend(seeded);
        let graph = FakeGraph::queue(scripts);
        let answer = list_checks(&graph, &scope(), "c-1").expect("checks");
        assert_eq!(answer["total"], json!(6));
        assert_eq!(answer["compliant"], json!(0));
        assert_eq!(answer["compliance_rate"], json!(0.0));
        // a seeded check is a real one, so it is stored rather than re-created on the next read
        let (statement, params) = graph.last();
        assert!(statement.contains("CREATE"), "{statement}");
        assert_eq!(params["props"]["company_id"], json!("c-1"));
        assert_eq!(params["props"]["status"], json!("pending_review"));
        assert_eq!(params["props"]["regulation"], json!("Regulatory"));
    }

    #[test]
    fn a_report_over_no_checks_says_to_run_them_first_and_claims_no_failure() {
        let graph = FakeGraph::returning(vec![]);
        let answer = report(&graph, &scope(), "c-1").expect("report");
        assert_eq!(answer["compliance_rate"], json!(1.0));
        assert_eq!(answer["findings"], json!([]));
        assert_eq!(
            answer["recommendations"],
            json!(["Run compliance checks first"])
        );
        // reading a report must not seed anything
        assert!(!graph.last().0.contains("CREATE"));
    }

    #[test]
    fn a_report_counts_findings_and_carries_only_the_notes_that_exist() {
        let graph = FakeGraph::returning(vec![
            check("k-1", "compliant", ""),
            check("k-2", "warning", "Raise the LCR buffer"),
            check("k-3", "non_compliant", ""),
            check("k-4", "pending_review", ""),
        ]);
        let answer = report(&graph, &scope(), "c-1").expect("report");
        // one of four compliant, so the rate is over every check, reviewed or not
        assert_eq!(answer["compliance_rate"], json!(0.25));
        assert_eq!(answer["total_findings"], json!(2));
        assert_eq!(answer["recommendations"], json!(["Raise the LCR buffer"]));
    }

    #[test]
    fn a_verdict_writes_the_status_and_the_note_and_answers_with_the_verdict() {
        let graph = FakeGraph::queue(vec![
            vec![check("k-1", "pending_review", "")],
            vec![json!({"id": "k-1"})],
        ]);
        let answer = update_status(
            &graph,
            &scope(),
            &json!({"company_id": "c-1", "check_id": "k-1", "status": "compliant",
                    "remediation": "none needed", "checked_at": "2026-03-02T00:00:00"}),
        )
        .expect("verdict");
        assert_eq!(answer, json!({"check_id": "k-1", "status": "compliant"}));
        let (statement, params) = graph.last();
        assert!(statement.contains("SET x += $patch"), "{statement}");
        assert_eq!(params["patch"]["status"], json!("compliant"));
        assert_eq!(params["patch"]["remediation"], json!("none needed"));
    }

    #[test]
    fn a_verdict_the_service_does_not_know_is_refused_with_the_list() {
        let graph = FakeGraph::returning(vec![]);
        let refused = update_status(
            &graph,
            &scope(),
            &json!({"company_id": "c-1", "check_id": "k-1", "status": "fine"}),
        );
        assert!(refused.is_err());
    }

    #[test]
    fn a_check_in_another_book_is_missing_rather_than_refused() {
        let graph = FakeGraph::returning(vec![check("k-9", "compliant", "")]);
        let missing = update_status(
            &graph,
            &scope(),
            &json!({"company_id": "c-1", "check_id": "k-1", "status": "compliant"}),
        );
        assert!(matches!(missing, Err(StoreError::NotFound)));
    }
}
