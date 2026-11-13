#![forbid(unsafe_code)]
//! regulatory-compliance-service: the regulations a company is held to, what state each is in, and the dashboard
//! that adds them up.
//!
//! Behaviours kept as shipped, each pinned by a test:
//!
//! * a regulation arrives `pending_review` unless the caller names a state, and keeps the risk it was given, with
//!   a risk that is not named defaulting to medium;
//! * a listing is the caller's own regulations for one company, narrowed by framework when one is asked for and
//!   unfiltered when the framework asked for is the empty string;
//! * a status change refuses nothing: a state that is not one of the four is ignored and the regulation keeps the
//!   state it had, while a state that is one of them is taken, and either way the review date is stamped;
//! * a regulation that is not the caller's is missing rather than updated;
//! * the dashboard's rate is the compliant share of the total, but a company with NO regulations scores a full
//!   hundred rather than nothing - the shipped answer, kept because it is what the dashboard has always said;
//! * the critical list is every regulation that carries a high or critical risk and is not compliant.

use serde_json::{json, Value};

use vimbai_store::read::{field, refused, text};
use vimbai_store::{BookScope, Graph, RecordKind, Records, StoreError};

/// The regulations a company is held to.
pub const REGULATION: RecordKind = RecordKind::new("Regulation", "OWNS_REGULATION");

/// The four states a regulation can be in. The shipped enum spells not-applicable `n/a`.
pub const STATUSES: [&str; 4] = ["compliant", "non_compliant", "pending_review", "n/a"];

fn settled_status(record: &Value) -> String {
    let held = field(record, "status");
    if held.is_empty() {
        "pending_review".to_string()
    } else {
        held.to_string()
    }
}

fn answer(record: &Value) -> Value {
    let risk = {
        let held = field(record, "risk_if_non_compliant");
        if held.is_empty() {
            "medium".to_string()
        } else {
            held.to_string()
        }
    };
    json!({
        "id": field(record, "id"),
        "company_id": field(record, "company_id"),
        "regulation_name": field(record, "regulation_name"),
        "jurisdiction": field(record, "jurisdiction"),
        "framework": field(record, "framework"),
        "requirement": field(record, "requirement"),
        "status": settled_status(record),
        "last_reviewed": field(record, "last_reviewed"),
        "next_review_due": field(record, "next_review_due"),
        "risk_if_non_compliant": risk,
    })
}

/// `POST /regulations`: record a regulation.
pub fn create_regulation(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    if text(payload, "company_id").is_empty() {
        return Err(refused("company_id is required"));
    }
    if text(payload, "regulation_name").is_empty() {
        return Err(refused("regulation_name is required"));
    }
    let status = {
        let stated = text(payload, "status");
        if stated.is_empty() {
            "pending_review".to_string()
        } else {
            stated
        }
    };
    let risk = {
        let stated = text(payload, "risk_if_non_compliant");
        if stated.is_empty() {
            "medium".to_string()
        } else {
            stated
        }
    };
    let fields = json!({
        "id": vimbai_common::ids::service_id(),
        "company_id": text(payload, "company_id"),
        "regulation_name": text(payload, "regulation_name"),
        "jurisdiction": text(payload, "jurisdiction"),
        "framework": text(payload, "framework"),
        "requirement": text(payload, "requirement"),
        "status": status,
        "last_reviewed": text(payload, "last_reviewed"),
        "next_review_due": text(payload, "next_review_due"),
        "risk_if_non_compliant": risk,
    });
    let created = Records::new(graph, REGULATION).create(scope, &fields)?;
    Ok(answer(&created))
}

fn mine(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    company_id: &str,
) -> Result<Vec<Value>, StoreError> {
    let held = Records::new(graph, REGULATION).list(scope, 500)?;
    Ok(held
        .into_iter()
        .filter(|regulation| field(regulation, "company_id") == company_id)
        .collect())
}

/// `GET /regulations`: a company's regulations, narrowed by framework when one is named.
pub fn list_regulations(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let company_id = text(query, "company_id");
    if company_id.is_empty() {
        return Err(refused("company_id is required"));
    }
    let framework = text(query, "framework");
    let held = mine(graph, scope, &company_id)?;
    let kept: Vec<Value> = held
        .iter()
        .filter(|regulation| framework.is_empty() || field(regulation, "framework") == framework)
        .map(answer)
        .collect();
    Ok(Value::Array(kept))
}

/// `POST /regulations/{id}/update`: move a regulation's state and stamp the review.
pub fn update_regulation_status(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    regulation_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let company_id = text(payload, "company_id");
    let found = mine(graph, scope, &company_id)?;
    let current = found
        .iter()
        .find(|regulation| field(regulation, "id") == regulation_id)
        .ok_or(StoreError::NotFound)?;
    let asked = text(payload, "status");
    // an unrecognised state is ignored rather than refused, and the regulation keeps the state it had
    let settled = if STATUSES.contains(&asked.as_str()) {
        asked
    } else {
        settled_status(current)
    };
    let reviewed = {
        let stated = text(payload, "today");
        if stated.is_empty() {
            text(payload, "last_reviewed")
        } else {
            stated
        }
    };
    Records::new(graph, REGULATION).update(
        scope,
        regulation_id,
        &json!({"status": settled, "last_reviewed": reviewed}),
    )?;
    Ok(json!({
        "updated": true,
        "regulation_id": regulation_id,
        "status": settled,
    }))
}

/// `GET /dashboard`: what a company's regulations add up to.
pub fn compliance_dashboard(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let company_id = text(query, "company_id");
    if company_id.is_empty() {
        return Err(refused("company_id is required"));
    }
    let held = mine(graph, scope, &company_id)?;
    let counted = |wanted: &str| {
        held.iter()
            .filter(|regulation| settled_status(regulation) == wanted)
            .count()
    };
    let compliant = counted("compliant");
    let total = held.len();
    // a company with nothing recorded scores a full hundred, which is what the dashboard has always said
    let rate = if total == 0 {
        100.0
    } else {
        (compliant as f64 / total as f64 * 100.0 * 100.0).round() / 100.0
    };

    let mut frameworks: Vec<(String, u64, u64, u64)> = Vec::new();
    let mut jurisdictions: Vec<(String, u64, u64)> = Vec::new();
    for regulation in &held {
        let framework = field(regulation, "framework").to_string();
        let state = settled_status(regulation);
        match frameworks
            .iter_mut()
            .find(|(name, _, _, _)| *name == framework)
        {
            Some(entry) => {
                entry.1 += 1;
                if state == "compliant" {
                    entry.2 += 1;
                } else if state == "non_compliant" {
                    entry.3 += 1;
                }
            }
            None => frameworks.push((
                framework,
                1,
                u64::from(state == "compliant"),
                u64::from(state == "non_compliant"),
            )),
        }
        let jurisdiction = field(regulation, "jurisdiction").to_string();
        match jurisdictions
            .iter_mut()
            .find(|(name, _, _)| *name == jurisdiction)
        {
            Some(entry) => {
                entry.1 += 1;
                if state == "compliant" {
                    entry.2 += 1;
                }
            }
            None => jurisdictions.push((jurisdiction, 1, u64::from(state == "compliant"))),
        }
    }

    let mut by_framework = serde_json::Map::new();
    for (name, all, good, bad) in &frameworks {
        by_framework.insert(
            name.clone(),
            json!({"total": all, "compliant": good, "non_compliant": bad}),
        );
    }
    let by_jurisdiction: Vec<Value> = jurisdictions
        .iter()
        .map(|(name, all, good)| json!({"jurisdiction": name, "total": all, "compliant": good}))
        .collect();

    let critical: Vec<Value> = held
        .iter()
        .filter(|regulation| {
            let risk = field(regulation, "risk_if_non_compliant");
            (risk == "high" || risk == "critical") && settled_status(regulation) != "compliant"
        })
        .map(|regulation| {
            json!({
                "regulation": field(regulation, "regulation_name"),
                "jurisdiction": field(regulation, "jurisdiction"),
                "framework": field(regulation, "framework"),
                "status": settled_status(regulation),
                "risk": field(regulation, "risk_if_non_compliant"),
            })
        })
        .collect();

    Ok(json!({
        "company_id": company_id,
        "total_regulations": total,
        "compliant": compliant,
        "non_compliant": counted("non_compliant"),
        "pending": counted("pending_review"),
        "compliance_rate": rate,
        "by_framework": by_framework,
        "by_jurisdiction": by_jurisdiction,
        "critical_items": critical,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::MemoryGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn a_regulation(
        graph: &MemoryGraph,
        name: &str,
        framework: &str,
        jurisdiction: &str,
        risk: &str,
    ) -> Value {
        create_regulation(
            graph,
            &scope(),
            &json!({
                "company_id": "co-1",
                "regulation_name": name,
                "jurisdiction": jurisdiction,
                "framework": framework,
                "requirement": "File on time",
                "risk_if_non_compliant": risk,
            }),
        )
        .expect("a regulation")
    }

    #[test]
    fn a_regulation_arrives_pending_with_a_medium_risk() {
        let graph = MemoryGraph::new();
        let made = a_regulation(&graph, "AML filing", "AML", "ZW", "");
        assert_eq!(made["status"], json!("pending_review"));
        assert_eq!(made["risk_if_non_compliant"], json!("medium"));
        assert!(!made["id"].as_str().unwrap_or_default().is_empty());
    }

    #[test]
    fn a_listing_narrows_by_framework_and_only_shows_this_company() {
        let graph = MemoryGraph::new();
        a_regulation(&graph, "AML filing", "AML", "ZW", "high");
        a_regulation(&graph, "IFRS 9", "IFRS", "ZW", "medium");
        create_regulation(
            &graph,
            &scope(),
            &json!({
                "company_id": "co-2",
                "regulation_name": "Other company",
                "framework": "AML"
            }),
        )
        .expect("a regulation");

        let all =
            list_regulations(&graph, &scope(), &json!({"company_id": "co-1"})).expect("a listing");
        assert_eq!(all.as_array().map(|held| held.len()), Some(2));
        let narrowed = list_regulations(
            &graph,
            &scope(),
            &json!({"company_id": "co-1", "framework": "AML"}),
        )
        .expect("a listing");
        assert_eq!(narrowed.as_array().map(|held| held.len()), Some(1));
        assert_eq!(narrowed[0]["regulation_name"], json!("AML filing"));
    }

    #[test]
    fn a_state_change_takes_a_known_state_and_ignores_an_unknown_one() {
        let graph = MemoryGraph::new();
        let made = a_regulation(&graph, "AML filing", "AML", "ZW", "high");
        let id = field(&made, "id").to_string();

        let changed = update_regulation_status(
            &graph,
            &scope(),
            &id,
            &json!({"company_id": "co-1", "status": "compliant", "today": "2026-03-01"}),
        )
        .expect("a change");
        assert_eq!(changed["status"], json!("compliant"));

        // a state that is not one of the four leaves the regulation as it was, and says so
        let nonsense = update_regulation_status(
            &graph,
            &scope(),
            &id,
            &json!({"company_id": "co-1", "status": "nonsense", "today": "2026-04-01"}),
        )
        .expect("a change");
        assert_eq!(nonsense["status"], json!("compliant"));

        let listed =
            list_regulations(&graph, &scope(), &json!({"company_id": "co-1"})).expect("a listing");
        assert_eq!(listed[0]["last_reviewed"], json!("2026-04-01"));
    }

    #[test]
    fn another_books_regulation_is_missing() {
        let graph = MemoryGraph::new();
        let made = a_regulation(&graph, "AML filing", "AML", "ZW", "high");
        let theirs = update_regulation_status(
            &graph,
            &BookScope::new("user-2", "book-2"),
            field(&made, "id"),
            &json!({"company_id": "co-1", "status": "compliant"}),
        );
        assert!(theirs.is_err());
    }

    #[test]
    fn the_dashboard_adds_up_its_frameworks_and_names_what_is_critical() {
        let graph = MemoryGraph::new();
        let one = a_regulation(&graph, "AML filing", "AML", "ZW", "high");
        a_regulation(&graph, "IFRS 9", "IFRS", "ZW", "medium");
        a_regulation(&graph, "PCI", "PCI_DSS", "US", "critical");
        update_regulation_status(
            &graph,
            &scope(),
            field(&one, "id"),
            &json!({"company_id": "co-1", "status": "compliant", "today": "2026-03-01"}),
        )
        .expect("a change");

        let board = compliance_dashboard(&graph, &scope(), &json!({"company_id": "co-1"}))
            .expect("a board");
        assert_eq!(board["total_regulations"], json!(3));
        assert_eq!(board["compliant"], json!(1));
        assert_eq!(board["pending"], json!(2));
        // one of three compliant
        assert_eq!(board["compliance_rate"], json!(33.33));
        assert_eq!(board["by_framework"]["AML"]["compliant"], json!(1));
        assert_eq!(board["by_framework"]["AML"]["non_compliant"], json!(0));
        // the critical list holds the high risk pending regulation and the critical one, but not the settled AML
        assert_eq!(
            board["critical_items"].as_array().map(|held| held.len()),
            Some(1)
        );
        assert_eq!(board["critical_items"][0]["regulation"], json!("PCI"));
    }

    #[test]
    fn a_company_with_nothing_recorded_scores_a_hundred() {
        let graph = MemoryGraph::new();
        let board = compliance_dashboard(&graph, &scope(), &json!({"company_id": "co-1"}))
            .expect("a board");
        assert_eq!(board["total_regulations"], json!(0));
        // the shipped dashboard answers a full hundred rather than nothing here
        assert_eq!(board["compliance_rate"], json!(100.0));
        assert_eq!(board["critical_items"], json!([]));
    }
}
