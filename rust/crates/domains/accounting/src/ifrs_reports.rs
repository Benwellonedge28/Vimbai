#![forbid(unsafe_code)]
//! ifrs-reporting-service, on the store.
//!
//! IFRS reports with their figures and disclosure notes. A report carries the standard it is filed
//! under, its period, the payload, a status, and an index of the notes written against it.
//!
//! The rules, as shipped:
//!
//! * a report is one of five kinds: balance sheet, income statement, cash flow, equity changes, notes,
//! * adding a note also appends its number and title to the report's own disclosure index, so the report
//!   carries its notes without a second query,
//! * a report is approved from draft or reviewed only, and any other state is refused,
//! * listing notes for a report that does not exist answers an empty list rather than a refusal, while
//!   reading that report does refuse.
//!
//! Not the IFRS calculators: revenue recognition, share-based payment and financial instruments are
//! arithmetic and live in their own crates. This is the paperwork around them.

use serde_json::{json, Value};

use vimbai_store::read::{optional, text};
use vimbai_store::{
    created_record, BookScope, Graph, Records, StoreError, DISCLOSURE_NOTE, IFRS_REPORT,
};

/// The report kinds accepted.
pub const TYPES: [&str; 5] = [
    "balance_sheet",
    "income_statement",
    "cash_flow",
    "equity_changes",
    "notes",
];

/// `/reports`.
pub fn create_report<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let kind = text(payload, "report_type");
    if !TYPES.contains(&kind.as_str()) {
        return Err(StoreError::Invalid(format!(
            "Invalid report type. Must be one of {:?}",
            TYPES
        )));
    }
    Records::new(graph, IFRS_REPORT).create(
        scope,
        &json!({
            "report_type": kind,
            "ifrs_standard": text(payload, "ifrs_standard"),
            "period": text(payload, "period"),
            "reporting_date": text(payload, "reporting_date"),
            "data": payload.get("data").cloned().unwrap_or_else(|| json!({})),
            "disclosures": [],
            "status": "draft",
            "approved_by": "",
        }),
    )
}

/// `/reports` listing, filtered as asked.
pub fn list_reports<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let kind = optional(query, "report_type");
    let standard = optional(query, "ifrs_standard");
    let status = optional(query, "status");
    let rows = Records::new(graph, IFRS_REPORT).list(scope, 1000)?;
    let kept: Vec<Value> = rows
        .into_iter()
        .filter(|row| {
            kind.as_deref()
                .map(|want| row.get("report_type").and_then(Value::as_str) == Some(want))
                .unwrap_or(true)
                && standard
                    .as_deref()
                    .map(|want| row.get("ifrs_standard").and_then(Value::as_str) == Some(want))
                    .unwrap_or(true)
                && status
                    .as_deref()
                    .map(|want| row.get("status").and_then(Value::as_str) == Some(want))
                    .unwrap_or(true)
        })
        .collect();
    Ok(Value::Array(kept))
}

/// `/reports/{id}`.
pub fn get_report<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    report_id: &str,
) -> Result<Value, StoreError> {
    Records::new(graph, IFRS_REPORT).get(scope, report_id)
}

/// `/reports/{id}/notes`.
pub fn add_note<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    report_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let reports = Records::new(graph, IFRS_REPORT);
    // reading the report first, so a note cannot be written against one that is not there
    let report = reports.get(scope, report_id)?;

    let note_number = payload
        .get("note_number")
        .and_then(Value::as_i64)
        .unwrap_or_default();
    let title = text(payload, "title");

    let note = Records::new(graph, DISCLOSURE_NOTE).create(
        scope,
        &json!({
            "report_id": report_id,
            "note_number": note_number,
            "title": title,
            "content": text(payload, "content"),
            "ifrs_reference": text(payload, "ifrs_reference"),
        }),
    )?;

    // the report carries the index of its own notes
    let mut disclosures = report
        .get("disclosures")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    disclosures.push(json!({"note_number": note_number, "title": title}));
    reports.update(scope, report_id, &json!({"disclosures": disclosures}))?;

    Ok(created_record(&note))
}

/// `/reports/{id}/notes` listing: an unknown report answers empty rather than refusing.
pub fn list_notes<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    report_id: &str,
) -> Result<Value, StoreError> {
    let rows = Records::new(graph, DISCLOSURE_NOTE).list(scope, 1000)?;
    let mut kept: Vec<Value> = rows
        .into_iter()
        .filter(|row| row.get("report_id").and_then(Value::as_str) == Some(report_id))
        .collect();
    kept.sort_by_key(|row| {
        row.get("note_number")
            .and_then(Value::as_i64)
            .unwrap_or_default()
    });
    Ok(Value::Array(kept))
}

/// `/reports/{id}/approve`.
pub fn approve_report<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    report_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let reports = Records::new(graph, IFRS_REPORT);
    let report = reports.get(scope, report_id)?;
    let status = report
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("draft");
    if status != "draft" && status != "reviewed" {
        return Err(StoreError::Invalid(format!(
            "Report cannot be approved from {status} state"
        )));
    }
    let approved_by = text(payload, "approved_by");
    reports.update(
        scope,
        report_id,
        &json!({"status": "approved", "approved_by": approved_by}),
    )?;
    Ok(json!({
        "report_id": report_id,
        "status": "approved",
        "approved_by": approved_by,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::FakeGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn report(id: &str, status: &str, disclosures: Value) -> Value {
        json!({"id": id, "report_type": "balance_sheet", "ifrs_standard": "IAS1",
               "period": "2026-03", "status": status, "disclosures": disclosures})
    }

    #[test]
    fn only_the_five_report_kinds_are_accepted() {
        for kind in TYPES {
            let graph = FakeGraph::returning(vec![json!({"id": "r-1"})]);
            create_report(
                &graph,
                &scope(),
                &json!({"report_type": kind, "ifrs_standard": "IAS1", "period": "2026-03",
                        "reporting_date": "2026-03-31"}),
            )
            .expect("accepted");
        }
        let graph = FakeGraph::returning(vec![json!({"id": "r-1"})]);
        assert!(matches!(
            create_report(
                &graph,
                &scope(),
                &json!({"report_type": "trial_balance", "ifrs_standard": "IAS1",
                        "period": "2026-03", "reporting_date": "2026-03-31"})
            ),
            Err(StoreError::Invalid(_))
        ));
    }

    #[test]
    fn a_new_report_is_a_draft_with_an_empty_disclosure_index() {
        let graph = FakeGraph::returning(vec![json!({"id": "r-1"})]);
        create_report(
            &graph,
            &scope(),
            &json!({"report_type": "notes", "ifrs_standard": "IFRS15", "period": "2026",
                    "reporting_date": "2026-12-31", "data": {"revenue": "100.55"}}),
        )
        .expect("created");
        let (_, params) = graph.last();
        assert_eq!(params["props"]["status"], json!("draft"));
        assert_eq!(params["props"]["disclosures"], json!([]));
        // the payload is kept as given, exact strings included
        assert_eq!(params["props"]["data"]["revenue"], json!("100.55"));
    }

    #[test]
    fn a_note_also_lands_in_the_reports_own_index() {
        let graph = FakeGraph::queue(vec![
            vec![report(
                "r-1",
                "draft",
                json!([{"note_number": 1, "title": "Basis"}]),
            )],
            vec![json!({"id": "n-2"})],
            vec![json!({"id": "r-1"})],
        ]);
        let note = add_note(
            &graph,
            &scope(),
            "r-1",
            &json!({"note_number": 2, "title": "Revenue", "content": "Recognised on delivery",
                    "ifrs_reference": "IFRS15"}),
        )
        .expect("added");
        assert_eq!(note["id"], json!("n-2"));
        let (_, patch) = graph.statement(2);
        let index = patch["patch"]["disclosures"].as_array().expect("index");
        assert_eq!(index.len(), 2);
        assert_eq!(index[1]["note_number"], json!(2));
        assert_eq!(index[1]["title"], json!("Revenue"));
    }

    #[test]
    fn a_note_against_a_report_that_is_not_there_is_refused() {
        let graph = FakeGraph::queue(vec![vec![]]);
        assert!(matches!(
            add_note(
                &graph,
                &scope(),
                "gone",
                &json!({"note_number": 1, "title": "x"})
            ),
            Err(StoreError::NotFound)
        ));
    }

    #[test]
    fn listing_notes_for_an_unknown_report_is_an_empty_list() {
        let graph = FakeGraph::returning(vec![
            json!({"report_id": "r-1", "note_number": 2}),
            json!({"report_id": "r-1", "note_number": 1}),
        ]);
        let empty = list_notes(&graph, &scope(), "gone").expect("listed");
        assert_eq!(empty, json!([]));
        let ordered = list_notes(&graph, &scope(), "r-1").expect("listed");
        assert_eq!(ordered[0]["note_number"], json!(1));
    }

    #[test]
    fn a_report_is_approved_from_draft_or_reviewed_only() {
        for state in ["draft", "reviewed"] {
            let graph = FakeGraph::queue(vec![
                vec![report("r-1", state, json!([]))],
                vec![report("r-1", "approved", json!([]))],
            ]);
            let answer =
                approve_report(&graph, &scope(), "r-1", &json!({"approved_by": "auditor"}))
                    .expect("approved");
            assert_eq!(answer["status"], json!("approved"));
            assert_eq!(answer["approved_by"], json!("auditor"));
        }
        for state in ["approved", "published"] {
            let graph = FakeGraph::queue(vec![vec![report("r-1", state, json!([]))]]);
            assert!(matches!(
                approve_report(&graph, &scope(), "r-1", &json!({"approved_by": "auditor"})),
                Err(StoreError::Invalid(_))
            ));
        }
    }
}
