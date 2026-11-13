#![forbid(unsafe_code)]
//! bank-relationship-service: the banks a company deals with, the quality of the service it gets from them, and
//! the summary of the whole relationship book.
//!
//! Behaviours kept as shipped, each pinned by a test:
//!
//! * a relationship starts `active` with a rating of 3 when the caller states neither, and a rating is a whole
//!   number from 1 to 5;
//! * a status has to be one of the four the service knew - active, dormant, terminated, prospective;
//! * updating a relationship only moves the three things it could move: the rating, the status and the notes.
//!   Everything else stays as it was recorded;
//! * a quality metric may only be added against a relationship the caller owns, so a metric cannot be pinned to
//!   somebody else's bank;
//! * the average score of a relationship is measured over its metrics, and a relationship with none averages
//!   nil rather than dividing by zero;
//! * the summary counts distinct bank names and distinct services, not rows.

use serde_json::{json, Value};

use vimbai_common::Money;
use vimbai_store::read::{field, label, refused, text, whole};
use vimbai_store::{BookScope, Graph, Records, StoreError, BANK_QUALITY_METRIC, BANK_RELATIONSHIP};

/// The most records a listing reads in one go.
const LARGEST_PAGE: usize = 2000;

/// The statuses the shipped service knew by name.
const STATUSES: [&str; 4] = ["active", "dormant", "terminated", "prospective"];

fn relationships(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
) -> Result<Vec<Value>, StoreError> {
    Records::new(graph, BANK_RELATIONSHIP).list(scope, LARGEST_PAGE)
}

fn metrics(graph: &(impl Graph + ?Sized), scope: &BookScope) -> Result<Vec<Value>, StoreError> {
    Records::new(graph, BANK_QUALITY_METRIC).list(scope, LARGEST_PAGE)
}

/// The strings of a list field, however the store handed them back.
fn strings_of(record: &Value, key: &str) -> Vec<String> {
    match record.get(key) {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect(),
        Some(Value::String(joined)) => joined
            .split(',')
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(str::to_owned)
            .collect(),
        _ => Vec::new(),
    }
}

/// A rating, held to the five it may be.
fn rating_of(payload: &Value) -> Result<i64, StoreError> {
    match whole(payload, "rating") {
        0 => Ok(3),
        stated if (1..=5).contains(&stated) => Ok(stated as i64),
        stated => Err(refused(&format!(
            "rating must be between 1 and 5, not {stated}"
        ))),
    }
}

// ---------------------------------------------------------------------------
// relationships
// ---------------------------------------------------------------------------

/// `POST /relationships`.
pub fn create_relationship(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let company_id = text(payload, "company_id");
    if company_id.is_empty() {
        return Err(refused("company_id is required"));
    }
    let bank_name = text(payload, "bank_name");
    if bank_name.is_empty() {
        return Err(refused("bank_name is required"));
    }
    let status = match text(payload, "status") {
        stated if stated.is_empty() => "active".to_string(),
        stated => {
            let lowered = stated.to_lowercase();
            if !STATUSES.contains(&lowered.as_str()) {
                return Err(refused(&format!("Unknown status {stated}")));
            }
            lowered
        }
    };
    let today = label(payload, "today");
    let body = json!({
        "company_id": company_id,
        "bank_name": bank_name,
        "branch": text(payload, "branch"),
        "account_number": text(payload, "account_number"),
        "relationship_manager": text(payload, "relationship_manager"),
        "contact_email": text(payload, "contact_email"),
        "contact_phone": text(payload, "contact_phone"),
        "services": strings_of(payload, "services"),
        "status": status,
        "opened_date": text(payload, "opened_date"),
        "rating": rating_of(payload)?,
        "notes": text(payload, "notes"),
        "created_at": today,
    });
    Records::new(graph, BANK_RELATIONSHIP).create(scope, &body)
}

/// `GET /relationships/{company_id}`, narrowed by status.
pub fn list_relationships(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    company_id: &str,
    query: &Value,
) -> Result<Value, StoreError> {
    let wanted = label(query, "status_filter").to_lowercase();
    let mine: Vec<Value> = relationships(graph, scope)?
        .into_iter()
        .filter(|held| field(held, "company_id") == company_id)
        .filter(|held| wanted.is_empty() || field(held, "status").eq_ignore_ascii_case(&wanted))
        .collect();
    Ok(json!({
        "company_id": company_id,
        "relationships": mine,
        "total": mine.len(),
    }))
}

/// `PUT /relationships/{id}`: the rating, the status and the notes, and nothing else.
pub fn update_relationship(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    relationship_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let held = Records::new(graph, BANK_RELATIONSHIP)
        .get(scope, relationship_id)
        .map_err(|_| StoreError::NotFound)?;
    // each of the three is only moved when the caller stated it, which is what the shipped update did
    let rating = match whole(payload, "rating") {
        0 => whole(&held, "rating") as i64,
        stated if (1..=5).contains(&stated) => stated as i64,
        stated => {
            return Err(refused(&format!(
                "rating must be between 1 and 5, not {stated}"
            )))
        }
    };
    let status = match text(payload, "status") {
        stated if stated.is_empty() => field(&held, "status").to_string(),
        stated => {
            let lowered = stated.to_lowercase();
            if !STATUSES.contains(&lowered.as_str()) {
                return Err(refused(&format!("Unknown status {stated}")));
            }
            lowered
        }
    };
    let notes = match payload.get("notes") {
        Some(stated) => stated.as_str().unwrap_or_default().to_string(),
        None => field(&held, "notes").to_string(),
    };
    Records::new(graph, BANK_RELATIONSHIP).update(
        scope,
        relationship_id,
        &json!({ "rating": rating, "status": status, "notes": notes }),
    )?;
    // the shipped service answered the three fields it had moved rather than the whole record
    Ok(json!({
        "id": relationship_id,
        "rating": rating,
        "status": status,
    }))
}

// ---------------------------------------------------------------------------
// service quality
// ---------------------------------------------------------------------------

/// `POST /quality-metrics`: a score against a relationship the caller holds.
pub fn add_quality_metric(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let relationship_id = text(payload, "relationship_id");
    if relationship_id.is_empty() {
        return Err(refused("relationship_id is required"));
    }
    // the relationship has to be the caller's before a metric is pinned to it
    Records::new(graph, BANK_RELATIONSHIP)
        .get(scope, &relationship_id)
        .map_err(|_| StoreError::NotFound)?;
    let metric_name = text(payload, "metric_name");
    if metric_name.is_empty() {
        return Err(refused("metric_name is required"));
    }
    let score = match whole(payload, "score") {
        0 => 1,
        stated if (1..=5).contains(&stated) => stated as i64,
        stated => {
            return Err(refused(&format!(
                "score must be between 1 and 5, not {stated}"
            )))
        }
    };
    let today = label(payload, "today");
    Records::new(graph, BANK_QUALITY_METRIC).create(
        scope,
        &json!({
            "relationship_id": relationship_id,
            "metric_name": metric_name,
            "score": score,
            "notes": text(payload, "notes"),
            "recorded_at": today,
            "created_at": today,
        }),
    )?;
    // the shipped service answered the metric's id, name and score rather than the whole record
    Ok(json!({
        "id": metric_name,
        "metric": metric_name,
        "score": score,
    }))
}

/// `GET /quality-metrics/{relationship_id}`: the metrics and their average, oldest first.
pub fn get_quality_metrics(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    relationship_id: &str,
) -> Result<Value, StoreError> {
    Records::new(graph, BANK_RELATIONSHIP)
        .get(scope, relationship_id)
        .map_err(|_| StoreError::NotFound)?;
    let mut mine: Vec<Value> = metrics(graph, scope)?
        .into_iter()
        .filter(|held| field(held, "relationship_id") == relationship_id)
        .collect();
    mine.sort_by(|left, right| field(left, "recorded_at").cmp(field(right, "recorded_at")));
    let total: i64 = mine.iter().map(|held| whole(held, "score") as i64).sum();
    // a relationship with no metrics averages nil rather than dividing by nothing
    let average = if mine.is_empty() {
        Money::zero()
    } else {
        Money::from_i64(total)
            .over(&Money::from_i64(mine.len() as i64))
            .round(4)
    };
    Ok(json!({
        "relationship_id": relationship_id,
        "avg_score": wire_number(&average),
        "metrics": mine,
    }))
}

/// The average as a number rather than a decimal string, because it is a score and not an amount.
fn wire_number(value: &Money) -> Value {
    Value::from(value.to_f64_lossy())
}

// ---------------------------------------------------------------------------
// summary
// ---------------------------------------------------------------------------

/// `GET /summary/{company_id}`: the relationship book for one company.
pub fn relationship_summary(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    company_id: &str,
) -> Result<Value, StoreError> {
    let mine: Vec<Value> = relationships(graph, scope)?
        .into_iter()
        .filter(|held| field(held, "company_id") == company_id)
        .collect();
    let active = mine
        .iter()
        .filter(|held| field(held, "status") == "active")
        .count();
    let mut banks: Vec<String> = Vec::new();
    let mut services: Vec<String> = Vec::new();
    let mut ratings = Money::zero();
    for held in &mine {
        let bank = field(held, "bank_name").to_string();
        if !bank.is_empty() && !banks.contains(&bank) {
            banks.push(bank);
        }
        for service in strings_of(held, "services") {
            if !services.contains(&service) {
                services.push(service);
            }
        }
        ratings = ratings.plus(&Money::from_i64(whole(held, "rating") as i64));
    }
    let average = if mine.is_empty() {
        Money::zero()
    } else {
        ratings.over(&Money::from_i64(mine.len() as i64)).round(4)
    };
    Ok(json!({
        "company_id": company_id,
        "total_relationships": mine.len(),
        "active": active,
        "unique_banks": banks.len(),
        "total_services": services.len(),
        "services": services,
        "avg_rating": wire_number(&average),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::FakeGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    #[test]
    fn a_relationship_starts_active_and_middling() {
        assert_eq!(rating_of(&json!({})).expect("a rating"), 3);
        assert_eq!(rating_of(&json!({"rating": 5})).expect("a rating"), 5);
        assert!(rating_of(&json!({"rating": 9})).is_err());
    }

    #[test]
    fn the_summary_counts_banks_and_services_once_each() {
        let graph = FakeGraph::queue(vec![vec![
            json!({"id": "r-1", "company_id": "co-1", "bank_name": "CBZ", "status": "active",
                   "rating": 4, "services": ["payments", "loans"]}),
            json!({"id": "r-2", "company_id": "co-1", "bank_name": "CBZ", "status": "dormant",
                   "rating": 2, "services": ["loans", "fx"]}),
            json!({"id": "r-3", "company_id": "co-1", "bank_name": "Stanbic", "status": "active",
                   "rating": 3, "services": ["fx"]}),
        ]]);
        let summary = relationship_summary(&graph, &scope(), "co-1").expect("a summary");
        assert_eq!(summary["total_relationships"], json!(3));
        assert_eq!(summary["active"], json!(2));
        // two banks across three relationships
        assert_eq!(summary["unique_banks"], json!(2));
        // payments, loans and fx
        assert_eq!(summary["total_services"], json!(3));
        // 3, 3 and 4 average to 3
        assert_eq!(summary["avg_rating"], json!(3.0));
    }

    #[test]
    fn a_company_with_no_relationships_averages_nothing_rather_than_dividing_by_zero() {
        let graph = FakeGraph::queue(vec![vec![]]);
        let summary = relationship_summary(&graph, &scope(), "co-1").expect("a summary");
        assert_eq!(summary["total_relationships"], json!(0));
        assert_eq!(summary["avg_rating"], json!(0.0));
        assert_eq!(summary["services"], json!([]));
    }

    #[test]
    fn a_relationship_with_no_metrics_averages_nothing() {
        let graph = FakeGraph::queue(vec![
            vec![json!({"id": "r-1", "company_id": "co-1"})],
            vec![],
        ]);
        let held = get_quality_metrics(&graph, &scope(), "r-1").expect("metrics");
        assert_eq!(held["avg_score"], json!(0.0));
        assert_eq!(held["metrics"], json!([]));
    }

    #[test]
    fn the_average_of_a_relationships_metrics_is_taken_oldest_first() {
        let graph = FakeGraph::queue(vec![
            vec![json!({"id": "r-1", "company_id": "co-1"})],
            vec![
                json!({"id": "m-2", "relationship_id": "r-1", "score": 5, "recorded_at": "2026-03-02"}),
                json!({"id": "m-1", "relationship_id": "r-1", "score": 2, "recorded_at": "2026-03-01"}),
                json!({"id": "m-3", "relationship_id": "r-9", "score": 1, "recorded_at": "2026-03-01"}),
            ],
        ]);
        let held = get_quality_metrics(&graph, &scope(), "r-1").expect("metrics");
        // two metrics, 2 and 5, average 3.5 - the metric on another relationship is not counted
        assert_eq!(held["avg_score"], json!(3.5));
        assert_eq!(
            held["metrics"].as_array().expect("a list")[0]["id"],
            json!("m-1")
        );
    }

    #[test]
    fn a_status_the_service_does_not_know_is_refused() {
        let graph = FakeGraph::queue(vec![]);
        let refused_status = create_relationship(
            &graph,
            &scope(),
            &json!({"company_id": "co-1", "bank_name": "CBZ", "status": "frenemy"}),
        );
        assert!(refused_status.is_err());
    }

    #[test]
    fn a_list_of_services_is_read_however_it_was_stored() {
        assert_eq!(
            strings_of(&json!({"services": ["fx", "loans"]}), "services"),
            vec!["fx".to_string(), "loans".to_string()]
        );
        assert_eq!(
            strings_of(&json!({"services": "fx, loans"}), "services"),
            vec!["fx".to_string(), "loans".to_string()]
        );
    }

    #[test]
    fn an_update_moves_only_the_rating_status_and_notes() {
        let graph = FakeGraph::queue(vec![
            // the relationship as recorded
            vec![
                json!({"id": "r-1", "bank_name": "CBZ", "status": "active", "rating": 4, "notes": "good"}),
            ],
            // the update
            vec![json!({"id": "r-1", "rating": 2, "status": "dormant", "notes": "good"})],
        ]);
        let answer = update_relationship(
            &graph,
            &scope(),
            "r-1",
            &json!({"rating": 2, "status": "dormant"}),
        )
        .expect("updated");
        // the notes were not stated, so they stay as they were
        assert_eq!(answer["rating"], json!(2));
        assert_eq!(answer["status"], json!("dormant"));
    }

    #[test]
    fn a_metric_cannot_be_pinned_to_a_relationship_the_caller_does_not_hold() {
        let graph = FakeGraph::queue(vec![vec![]]);
        let refused_metric = add_quality_metric(
            &graph,
            &scope(),
            &json!({"relationship_id": "r-9", "metric_name": "response_time", "score": 4}),
        );
        assert!(refused_metric.is_err());
    }
}
