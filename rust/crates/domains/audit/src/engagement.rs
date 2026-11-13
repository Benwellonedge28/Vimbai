#![forbid(unsafe_code)]
//! audit-engagement-service: the engagements an auditor runs, the findings raised during them, and the report
//! those findings add up to.
//!
//! An engagement is a record on the store, Book-scoped. Its findings travel with it as a property rather than as
//! separate nodes, because a finding is only ever read back with its engagement and never on its own.
//!
//! Behaviours kept as shipped, each pinned by a test:
//!
//! * an engagement starts `planned` with no findings and no summary, and its start date is the caller's date;
//! * the four states are planned, in progress, completed and cancelled, and any other word is refused;
//! * completing an engagement stamps its end date and takes the summary only in that state - a summary offered
//!   while moving to any other state is ignored rather than stored, as shipped;
//! * a finding defaults to medium severity, and adding one answers just the engagement, the finding and its
//!   severity rather than the whole record;
//! * a finding can be remediated by its own id, found among the caller's engagements; one that is not the
//!   caller's is missing;
//! * the report counts critical, high, medium and low findings but NOT informative ones, while its total counts
//!   every finding - so the four counts can add to less than the total, which is what the shipped report said.

use serde_json::{json, Value};

use vimbai_store::read::{field, refused, text};
use vimbai_store::{BookScope, Graph, Records, StoreError, AUDIT_ENGAGEMENT};

/// The states an engagement moves through.
pub const STATES: &[&str] = &["planned", "in_progress", "completed", "cancelled"];

/// The audit families this service covers.
pub const AUDIT_TYPES: &[&str] = &["tax", "forensic", "it", "operational"];

/// The severities a finding may carry.
pub const SEVERITIES: &[&str] = &["info", "low", "medium", "high", "critical"];

fn or_stated(record: &Value, key: &str, fallback: &str) -> Value {
    match record.get(key) {
        Some(Value::Null) | None => json!(fallback),
        Some(held) => held.clone(),
    }
}

fn findings_of(record: &Value) -> Vec<Value> {
    match record.get("findings") {
        Some(Value::Array(held)) => held.clone(),
        _ => Vec::new(),
    }
}

fn engagement_of(record: &Value) -> Value {
    json!({
        "id": field(record, "id"),
        "company_id": field(record, "company_id"),
        "audit_type": or_stated(record, "audit_type", "operational"),
        "title": field(record, "title"),
        "scope": field(record, "scope"),
        "objectives": record.get("objectives").cloned().unwrap_or_else(|| json!([])),
        "start_date": field(record, "start_date"),
        "end_date": record.get("end_date").cloned().unwrap_or(Value::Null),
        "auditor": field(record, "auditor"),
        "status": or_stated(record, "status", "planned"),
        "findings": findings_of(record),
        "summary": field(record, "summary"),
        "created_at": field(record, "created_at"),
    })
}

/// `GET /audit-types`: the families this service covers.
pub fn audit_types() -> Value {
    json!({
        "audit_types": AUDIT_TYPES,
        "note": "Pass audit_type to /engagements/{company_id} to read a single family.",
    })
}

/// `POST /engagements`: open an engagement.
pub fn create_engagement(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let audit_type = {
        let stated = text(payload, "audit_type");
        if stated.is_empty() {
            "operational".to_string()
        } else {
            stated
        }
    };
    if !AUDIT_TYPES.contains(&audit_type.as_str()) {
        return Err(refused(&format!(
            "Invalid audit_type. Must be one of {AUDIT_TYPES:?}"
        )));
    }
    let recorded = text(payload, "today");
    let start_date = {
        let stated = text(payload, "start_date");
        if stated.is_empty() {
            recorded.clone()
        } else {
            stated
        }
    };
    let mut fields = json!({
        "company_id": text(payload, "company_id"),
        "audit_type": audit_type,
        "title": text(payload, "title"),
        "scope": text(payload, "scope"),
        "objectives": payload.get("objectives").cloned().unwrap_or_else(|| json!([])),
        "start_date": start_date,
        "end_date": payload.get("end_date").cloned().unwrap_or(Value::Null),
        "auditor": text(payload, "auditor"),
        "status": "planned",
        "findings": json!([]),
        "summary": "",
        "created_at": recorded,
    });
    if let Some(stated) = payload.get("id").and_then(Value::as_str) {
        if !stated.trim().is_empty() {
            fields["id"] = json!(stated.trim());
        }
    }
    let created = Records::new(graph, AUDIT_ENGAGEMENT).create(scope, &fields)?;
    Ok(engagement_of(&created))
}

/// `GET /engagements/{company_id}`: a company's engagements, narrowed by state or family.
pub fn list_engagements(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    company_id: &str,
    query: &Value,
) -> Result<Value, StoreError> {
    let status = text(query, "status");
    let audit_type = text(query, "audit_type");
    let held = Records::new(graph, AUDIT_ENGAGEMENT).list(scope, 500)?;
    let mut mine: Vec<Value> = held.iter().map(engagement_of).collect();
    mine.retain(|row| field(row, "company_id") == company_id);
    if !status.is_empty() {
        mine.retain(|row| field(row, "status") == status);
    }
    if !audit_type.is_empty() {
        mine.retain(|row| field(row, "audit_type") == audit_type);
    }
    Ok(json!({
        "company_id": company_id,
        "engagements": mine,
        "total": mine.len(),
    }))
}

/// `PUT /engagements/{id}/status`: move an engagement on.
pub fn move_engagement(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    engagement_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let held = Records::new(graph, AUDIT_ENGAGEMENT).get(scope, engagement_id)?;
    let status = text(payload, "status");
    if !STATES.contains(&status.as_str()) {
        return Err(refused(&format!(
            "Invalid status. Must be one of {STATES:?}"
        )));
    }
    let recorded = text(payload, "today");
    let mut changed = json!({ "status": status });
    if status == "completed" {
        // completing is what closes an engagement, and only then does the summary take
        changed["end_date"] = json!(recorded);
        let summary = text(payload, "summary");
        if !summary.is_empty() {
            changed["summary"] = json!(summary);
        }
    } else {
        // the rest of the record is rewritten as it stands, so nothing else about it moves
        changed["end_date"] = held.get("end_date").cloned().unwrap_or(Value::Null);
        changed["summary"] = json!(field(&held, "summary"));
    }
    Records::new(graph, AUDIT_ENGAGEMENT).update(scope, engagement_id, &changed)?;
    Ok(json!({"id": engagement_id, "status": status}))
}

/// `POST /engagements/{id}/findings`: raise a finding during an engagement.
pub fn add_finding(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    engagement_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let held = Records::new(graph, AUDIT_ENGAGEMENT).get(scope, engagement_id)?;
    let severity = {
        let stated = text(payload, "severity");
        if stated.is_empty() {
            "medium".to_string()
        } else {
            stated
        }
    };
    if !SEVERITIES.contains(&severity.as_str()) {
        return Err(refused(&format!(
            "Invalid severity. Must be one of {SEVERITIES:?}"
        )));
    }
    let finding_id = {
        let stated = payload
            .get("finding_id")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if stated.trim().is_empty() {
            vimbai_common::ids::service_id()
        } else {
            stated.trim().to_string()
        }
    };
    let mut findings = findings_of(&held);
    findings.push(json!({
        "id": finding_id,
        "title": text(payload, "title"),
        "description": text(payload, "description"),
        "severity": severity,
        "recommendation": text(payload, "recommendation"),
        "status": "open",
        "evidence": text(payload, "evidence"),
    }));
    Records::new(graph, AUDIT_ENGAGEMENT).update(
        scope,
        engagement_id,
        &json!({ "findings": Value::Array(findings) }),
    )?;
    Ok(json!({
        "engagement_id": engagement_id,
        "finding_id": finding_id,
        "severity": severity,
    }))
}

/// `PUT /findings/{id}/remediate`: mark a finding remediated, wherever in the caller's engagements it sits.
pub fn remediate_finding(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    finding_id: &str,
) -> Result<Value, StoreError> {
    let held = Records::new(graph, AUDIT_ENGAGEMENT).list(scope, 500)?;
    for engagement in &held {
        let mut findings = findings_of(engagement);
        let mut found = false;
        for finding in findings.iter_mut() {
            if field(finding, "id") == finding_id {
                finding["status"] = json!("remediated");
                found = true;
            }
        }
        if !found {
            continue;
        }
        Records::new(graph, AUDIT_ENGAGEMENT).update(
            scope,
            field(engagement, "id"),
            &json!({ "findings": Value::Array(findings) }),
        )?;
        return Ok(json!({"finding_id": finding_id, "status": "remediated"}));
    }
    Err(StoreError::NotFound)
}

/// `GET /report/{id}`: what the engagement's findings add up to.
pub fn audit_report(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    engagement_id: &str,
) -> Result<Value, StoreError> {
    let held = Records::new(graph, AUDIT_ENGAGEMENT).get(scope, engagement_id)?;
    let findings = findings_of(&held);
    let counted = |wanted: &str| {
        findings
            .iter()
            .filter(|finding| field(finding, "severity") == wanted)
            .count()
    };
    let open = findings
        .iter()
        .filter(|finding| field(finding, "status") == "open")
        .count();
    let remediated = findings
        .iter()
        .filter(|finding| field(finding, "status") == "remediated")
        .count();
    Ok(json!({
        "engagement": engagement_of(&held),
        "findings_summary": {
            "critical": counted("critical"),
            "high": counted("high"),
            "medium": counted("medium"),
            "low": counted("low"),
            // informative findings are counted in the total but in none of the four above, as shipped
            "total": findings.len(),
        },
        "open_findings": open,
        "remediated": remediated,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::MemoryGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn an_engagement(graph: &MemoryGraph, scope: &BookScope, family: &str) -> Value {
        create_engagement(
            graph,
            scope,
            &json!({
                "company_id": "co-1",
                "audit_type": family,
                "title": "FY26 tax audit",
                "scope": "Corporation tax for the year",
                "objectives": ["Completeness", "Valuation"],
                "auditor": "Z. Moyo",
                "today": "2026-02-01T09:00:00Z"
            }),
        )
        .expect("an engagement")
    }

    #[test]
    fn an_engagement_starts_planned_with_no_findings() {
        let graph = MemoryGraph::new();
        let created = an_engagement(&graph, &scope(), "tax");
        assert_eq!(created["status"], json!("planned"));
        assert_eq!(created["findings"], json!([]));
        assert_eq!(created["summary"], json!(""));
        assert_eq!(created["start_date"], json!("2026-02-01T09:00:00Z"));
        assert_eq!(created["end_date"], Value::Null);
    }

    #[test]
    fn a_family_that_is_not_one_of_the_four_is_refused() {
        let graph = MemoryGraph::new();
        let refused_family = create_engagement(
            &graph,
            &scope(),
            &json!({"company_id": "co-1", "audit_type": "weather", "title": "Rain"}),
        );
        assert!(refused_family.is_err());
        // and no family at all is operational
        let defaulted = create_engagement(
            &graph,
            &scope(),
            &json!({"company_id": "co-1", "title": "Stock count", "today": "2026-02-02T09:00:00Z"}),
        )
        .expect("an engagement");
        assert_eq!(defaulted["audit_type"], json!("operational"));
    }

    #[test]
    fn a_summary_only_takes_when_the_engagement_is_completed() {
        let graph = MemoryGraph::new();
        let created = an_engagement(&graph, &scope(), "tax");
        let moved = move_engagement(
            &graph,
            &scope(),
            field(&created, "id"),
            &json!({"status": "in_progress", "summary": "Halfway", "today": "2026-03-01T09:00:00Z"}),
        )
        .expect("a move");
        assert_eq!(moved["status"], json!("in_progress"));
        let middling = list_engagements(&graph, &scope(), "co-1", &json!({})).expect("a listing");
        assert_eq!(middling["engagements"][0]["summary"], json!(""));

        move_engagement(
            &graph,
            &scope(),
            field(&created, "id"),
            &json!({"status": "completed", "summary": "Clean opinion", "today": "2026-04-01T09:00:00Z"}),
        )
        .expect("a move");
        let finished = list_engagements(&graph, &scope(), "co-1", &json!({})).expect("a listing");
        assert_eq!(
            finished["engagements"][0]["summary"],
            json!("Clean opinion")
        );
        assert_eq!(
            finished["engagements"][0]["end_date"],
            json!("2026-04-01T09:00:00Z")
        );
    }

    #[test]
    fn a_state_that_is_not_one_of_the_four_is_refused() {
        let graph = MemoryGraph::new();
        let created = an_engagement(&graph, &scope(), "tax");
        let refused_state = move_engagement(
            &graph,
            &scope(),
            field(&created, "id"),
            &json!({"status": "paused"}),
        );
        assert!(refused_state.is_err());
    }

    #[test]
    fn a_finding_defaults_to_medium_and_is_answered_by_its_id() {
        let graph = MemoryGraph::new();
        let created = an_engagement(&graph, &scope(), "tax");
        let raised = add_finding(
            &graph,
            &scope(),
            field(&created, "id"),
            &json!({
                "title": "Missing invoice",
                "description": "Two purchases have no supporting invoice",
                "recommendation": "Request copies from the supplier"
            }),
        )
        .expect("a finding");
        assert_eq!(raised["severity"], json!("medium"));
        let finding_id = field(&raised, "finding_id").to_string();
        assert!(!finding_id.is_empty());

        let reported = audit_report(&graph, &scope(), field(&created, "id")).expect("a report");
        assert_eq!(reported["findings_summary"]["medium"], json!(1));
        assert_eq!(reported["findings_summary"]["total"], json!(1));
        assert_eq!(reported["open_findings"], json!(1));
    }

    #[test]
    fn a_finding_is_remediated_by_its_own_id() {
        let graph = MemoryGraph::new();
        let created = an_engagement(&graph, &scope(), "tax");
        let raised = add_finding(
            &graph,
            &scope(),
            field(&created, "id"),
            &json!({"title": "Late filing", "description": "Return filed after the due date"}),
        )
        .expect("a finding");
        let remediated = remediate_finding(&graph, &scope(), field(&raised, "finding_id"))
            .expect("a remediation");
        assert_eq!(remediated["status"], json!("remediated"));
        let reported = audit_report(&graph, &scope(), field(&created, "id")).expect("a report");
        assert_eq!(reported["remediated"], json!(1));
        assert_eq!(reported["open_findings"], json!(0));
        // a finding that is not mine is missing
        assert!(remediate_finding(&graph, &BookScope::new("user-2", "book-2"), "no-such").is_err());
    }

    #[test]
    fn the_report_counts_the_four_severities_but_totals_every_finding() {
        let graph = MemoryGraph::new();
        let created = an_engagement(&graph, &scope(), "forensic");
        for (severity, title) in [
            ("critical", "Missing records"),
            ("high", "No dual approval"),
            ("info", "Filed neatly"),
        ] {
            add_finding(
                &graph,
                &scope(),
                field(&created, "id"),
                &json!({"title": title, "description": "Seen during fieldwork", "severity": severity}),
            )
            .expect("a finding");
        }
        let reported = audit_report(&graph, &scope(), field(&created, "id")).expect("a report");
        assert_eq!(reported["findings_summary"]["critical"], json!(1));
        assert_eq!(reported["findings_summary"]["high"], json!(1));
        assert_eq!(reported["findings_summary"]["medium"], json!(0));
        assert_eq!(reported["findings_summary"]["low"], json!(0));
        // three findings are counted in the total, two in the severities above
        assert_eq!(reported["findings_summary"]["total"], json!(3));
    }

    #[test]
    fn the_listing_narrows_by_state_and_family() {
        let graph = MemoryGraph::new();
        an_engagement(&graph, &scope(), "tax");
        an_engagement(&graph, &scope(), "it");
        let only_it = list_engagements(&graph, &scope(), "co-1", &json!({"audit_type": "it"}))
            .expect("a listing");
        assert_eq!(only_it["total"], json!(1));
        let none = list_engagements(&graph, &scope(), "co-1", &json!({"status": "completed"}))
            .expect("a listing");
        assert_eq!(none["total"], json!(0));
        assert_eq!(
            list_engagements(
                &graph,
                &BookScope::new("user-2", "book-2"),
                "co-1",
                &json!({})
            )
            .expect("a listing")["total"],
            json!(0)
        );
    }

    #[test]
    fn the_families_are_named_by_the_service() {
        assert_eq!(
            audit_types()["audit_types"],
            json!(["tax", "forensic", "it", "operational"])
        );
    }
}
