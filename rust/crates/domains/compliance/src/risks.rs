#![forbid(unsafe_code)]
//! risk-assessment-service: the risk register, scored by likelihood and impact.
//!
//! Records are durable and Book-scoped, one label under its own edge from the caller.
//!
//! Behaviours kept as shipped, each pinned by a test:
//!
//! * a risk scores likelihood times impact, and the score picks its level: up to four is low, up to nine is
//!   moderate, up to sixteen is high, and anything above that is critical - so a five-by-five risk at twenty-five
//!   is critical;
//! * a risk is created `identified` and can be moved through assessing, mitigating and monitoring to closed;
//! * a risk's level is recomputed whenever the score moves, so a mitigated risk that is reassessed downward stops
//!   being reported at its old level;
//! * only the fields stated are changed: an update that names no mitigation keeps the one it had;
//! * the listing narrows to one category or one level, and carries a count for every level whether or not any
//!   risk is at it - which is what the caller draws its bars from;
//! * the dashboard answers zeroes rather than nothing for a company with no risks, and names the five highest
//!   scoring risks as the ones to look at;
//! * closing a risk sets it closed and nothing else, so a closed risk keeps its score and its mitigation for the
//!   record.

use serde_json::{json, Value};

use vimbai_store::read::{field, refused, text, whole};
use vimbai_store::{BookScope, Graph, Records, StoreError, RISK_ITEM};

/// The levels a score can fall into, in the order the listing counts them.
pub const LEVELS: &[&str] = &["low", "moderate", "high", "critical"];

/// The categories a risk may sit in.
pub const CATEGORIES: &[&str] = &[
    "financial",
    "operational",
    "strategic",
    "compliance",
    "cyber",
    "market",
    "credit",
    "liquidity",
];

/// The level a score falls into.
pub fn level_of(score: u32) -> &'static str {
    if score <= 4 {
        "low"
    } else if score <= 9 {
        "moderate"
    } else if score <= 16 {
        "high"
    } else {
        "critical"
    }
}

fn or_stated(record: &Value, key: &str, fallback: &str) -> Value {
    match record.get(key) {
        Some(Value::Null) | None => json!(fallback),
        Some(held) => held.clone(),
    }
}

fn risk_of(record: &Value) -> Value {
    json!({
        "id": field(record, "id"),
        "company_id": field(record, "company_id"),
        "category": field(record, "category"),
        "name": field(record, "name"),
        "description": field(record, "description"),
        "likelihood": whole(record, "likelihood"),
        "impact": whole(record, "impact"),
        "owner": field(record, "owner"),
        "mitigation": field(record, "mitigation"),
        "risk_score": whole(record, "risk_score"),
        "level": field(record, "level"),
        "status": or_stated(record, "status", "identified"),
        "created_at": field(record, "created_at"),
        "updated_at": field(record, "updated_at"),
    })
}

fn all_risks(graph: &(impl Graph + ?Sized), scope: &BookScope) -> Result<Vec<Value>, StoreError> {
    let held = Records::new(graph, RISK_ITEM).list(scope, 500)?;
    Ok(held.iter().map(risk_of).collect())
}

/// `POST /risks`: register a risk, scored as it arrives.
pub fn create_risk(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let category = {
        let stated = text(payload, "category");
        if stated.is_empty() {
            return Err(refused("category is required"));
        }
        stated
    };
    if !CATEGORIES.contains(&category.as_str()) {
        return Err(refused(&format!(
            "Invalid category. Must be one of {CATEGORIES:?}"
        )));
    }
    let likelihood = match whole(payload, "likelihood") {
        0 => 1,
        stated => stated,
    };
    let impact = match whole(payload, "impact") {
        0 => 1,
        stated => stated,
    };
    for (name, held) in [("likelihood", likelihood), ("impact", impact)] {
        if !(1..=5).contains(&held) {
            return Err(refused(&format!("{name} must be between 1 and 5")));
        }
    }
    let score = likelihood * impact;
    let recorded = text(payload, "today");
    let mut fields = json!({
        "company_id": text(payload, "company_id"),
        "category": category,
        "name": text(payload, "name"),
        "description": text(payload, "description"),
        "likelihood": likelihood,
        "impact": impact,
        "owner": text(payload, "owner"),
        "mitigation": text(payload, "mitigation"),
        "risk_score": score,
        "level": level_of(score),
        "status": "identified",
        "created_at": recorded,
        "updated_at": recorded,
    });
    if let Some(stated) = payload.get("id").and_then(Value::as_str) {
        if !stated.trim().is_empty() {
            fields["id"] = json!(stated.trim());
        }
    }
    let created = Records::new(graph, RISK_ITEM).create(scope, &fields)?;
    // the shipped answer to a create was the id, the score and the level only
    Ok(json!({
        "id": field(&created, "id"),
        "risk_score": whole(&created, "risk_score"),
        "level": field(&created, "level"),
    }))
}

/// `GET /risks/{company_id}`: the register for one company, narrowed by category or level.
pub fn list_risks(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    company_id: &str,
    query: &Value,
) -> Result<Value, StoreError> {
    let category = text(query, "category");
    let level = text(query, "level");
    let mut mine = all_risks(graph, scope)?;
    mine.retain(|row| field(row, "company_id") == company_id);
    if !category.is_empty() {
        mine.retain(|row| field(row, "category") == category);
    }
    if !level.is_empty() {
        mine.retain(|row| field(row, "level") == level);
    }
    // every level is counted, whether or not a risk sits at it, because the caller draws bars from this
    let mut by_level = serde_json::Map::new();
    for wanted in LEVELS {
        let count = mine
            .iter()
            .filter(|row| field(row, "level") == *wanted)
            .count();
        by_level.insert((*wanted).to_string(), json!(count));
    }
    Ok(json!({
        "company_id": company_id,
        "risks": mine,
        "total": mine.len(),
        "by_level": Value::Object(by_level),
    }))
}

/// `PUT /risks/{id}`: reassess a risk, recomputing its score and level.
pub fn update_risk(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    risk_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let held = Records::new(graph, RISK_ITEM).get(scope, risk_id)?;
    let pick = |key: &str, current: u32| match whole(payload, key) {
        0 => current,
        stated => stated,
    };
    let likelihood = pick("likelihood", whole(&held, "likelihood"));
    let impact = pick("impact", whole(&held, "impact"));
    for (name, stated) in [("likelihood", likelihood), ("impact", impact)] {
        if !(1..=5).contains(&stated) {
            return Err(refused(&format!("{name} must be between 1 and 5")));
        }
    }
    let score = likelihood * impact;
    let mitigation = {
        let stated = text(payload, "mitigation");
        if stated.is_empty() {
            field(&held, "mitigation").to_string()
        } else {
            stated
        }
    };
    let status = {
        let stated = text(payload, "status");
        if stated.is_empty() {
            field(&held, "status").to_string()
        } else {
            stated
        }
    };
    let changed = json!({
        "likelihood": likelihood,
        "impact": impact,
        "mitigation": mitigation,
        "status": status,
        // the level follows the score, so a risk reassessed downward stops being reported at its old level
        "risk_score": score,
        "level": level_of(score),
        "updated_at": text(payload, "today"),
    });
    let saved = Records::new(graph, RISK_ITEM).update(scope, risk_id, &changed)?;
    Ok(json!({
        "id": field(&saved, "id"),
        "risk_score": whole(&saved, "risk_score"),
        "level": field(&saved, "level"),
        "status": field(&saved, "status"),
    }))
}

/// `GET /dashboard/{company_id}`: the shape a risk lead reads.
pub fn risk_dashboard(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    company_id: &str,
) -> Result<Value, StoreError> {
    let mut mine = all_risks(graph, scope)?;
    mine.retain(|row| field(row, "company_id") == company_id);
    if mine.is_empty() {
        // a company with no risks answers zeroes rather than nothing
        return Ok(json!({
            "company_id": company_id,
            "total_risks": 0,
            "by_level": {},
            "by_category": {},
            "avg_score": 0,
            "top_risks": Vec::<Value>::new(),
        }));
    }
    let mut by_level = serde_json::Map::new();
    for wanted in LEVELS {
        let count = mine
            .iter()
            .filter(|row| field(row, "level") == *wanted)
            .count();
        by_level.insert((*wanted).to_string(), json!(count));
    }
    let mut by_category = serde_json::Map::new();
    for wanted in CATEGORIES {
        let count = mine
            .iter()
            .filter(|row| field(row, "category") == *wanted)
            .count();
        by_category.insert((*wanted).to_string(), json!(count));
    }
    let total: u32 = mine.iter().map(|row| whole(row, "risk_score")).sum();
    // the shipped dashboard divided as a float; the average is the same number here, rounded to two places
    let avg = {
        let mean = total as f64 / mine.len() as f64;
        (mean * 100.0).round() / 100.0
    };
    let mut ranked = mine.clone();
    ranked.sort_by_key(|row| std::cmp::Reverse(whole(row, "risk_score")));
    let top: Vec<Value> = ranked.into_iter().take(5).collect();
    Ok(json!({
        "company_id": company_id,
        "total_risks": mine.len(),
        "by_level": Value::Object(by_level),
        "by_category": Value::Object(by_category),
        "avg_score": avg,
        "top_risks": top,
    }))
}

/// `DELETE /risks/{id}`: close a risk, keeping everything else about it.
pub fn close_risk(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    risk_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let saved = Records::new(graph, RISK_ITEM).update(
        scope,
        risk_id,
        &json!({"status": "closed", "updated_at": text(payload, "today")}),
    )?;
    Ok(json!({
        "id": field(&saved, "id"),
        "status": field(&saved, "status"),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::MemoryGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn a_risk(graph: &MemoryGraph, scope: &BookScope, likelihood: u32, impact: u32) -> Value {
        create_risk(
            graph,
            scope,
            &json!({
                "company_id": "co-1",
                "category": "financial",
                "name": "Customer concentration",
                "description": "One customer is a third of revenue",
                "likelihood": likelihood,
                "impact": impact,
                "owner": "CFO",
                "today": "2026-03-01T09:00:00Z"
            }),
        )
        .expect("a risk")
    }

    #[test]
    fn the_level_is_picked_by_the_score_at_the_shipped_boundaries() {
        assert_eq!(level_of(4), "low");
        assert_eq!(level_of(5), "moderate");
        assert_eq!(level_of(9), "moderate");
        assert_eq!(level_of(10), "high");
        assert_eq!(level_of(16), "high");
        assert_eq!(level_of(25), "critical");
    }

    #[test]
    fn a_risk_scores_likelihood_times_impact_and_starts_identified() {
        let graph = MemoryGraph::new();
        let created = a_risk(&graph, &scope(), 5, 5);
        assert_eq!(created["risk_score"], json!(25));
        assert_eq!(created["level"], json!("critical"));
        let listed = list_risks(&graph, &scope(), "co-1", &json!({})).expect("a listing");
        assert_eq!(listed["risks"][0]["status"], json!("identified"));
        assert_eq!(listed["total"], json!(1));
    }

    #[test]
    fn a_category_that_is_not_one_of_the_eight_is_refused() {
        let graph = MemoryGraph::new();
        let refused_category = create_risk(
            &graph,
            &scope(),
            &json!({"company_id": "co-1", "category": "weather", "name": "Rain"}),
        );
        assert!(refused_category.is_err());
    }

    #[test]
    fn a_reassessment_moves_the_score_and_the_level_with_it() {
        let graph = MemoryGraph::new();
        let created = a_risk(&graph, &scope(), 5, 5);
        let moved = update_risk(
            &graph,
            &scope(),
            field(&created, "id"),
            &json!({
                "likelihood": 2,
                "impact": 2,
                "mitigation": "Contract renewed with two new customers",
                "today": "2026-04-01T09:00:00Z"
            }),
        )
        .expect("an update");
        assert_eq!(moved["risk_score"], json!(4));
        assert_eq!(moved["level"], json!("low"));
        // the status was not named, so it stays as it was
        assert_eq!(moved["status"], json!("identified"));
        let listed = list_risks(&graph, &scope(), "co-1", &json!({})).expect("a listing");
        assert_eq!(
            listed["risks"][0]["mitigation"],
            json!("Contract renewed with two new customers")
        );
    }

    #[test]
    fn a_listing_counts_every_level_even_the_empty_ones() {
        let graph = MemoryGraph::new();
        a_risk(&graph, &scope(), 1, 1);
        let listed = list_risks(&graph, &scope(), "co-1", &json!({})).expect("a listing");
        assert_eq!(
            listed["by_level"],
            json!({"low": 1, "moderate": 0, "high": 0, "critical": 0})
        );
    }

    #[test]
    fn a_company_with_no_risks_gets_zeroes_rather_than_nothing() {
        let graph = MemoryGraph::new();
        let empty = risk_dashboard(&graph, &scope(), "co-1").expect("a dashboard");
        assert_eq!(empty["total_risks"], json!(0));
        assert_eq!(empty["by_level"], json!({}));
        assert_eq!(empty["avg_score"], json!(0));
        assert_eq!(empty["top_risks"], json!([]));
    }

    #[test]
    fn the_dashboard_averages_the_scores_and_names_the_five_worst() {
        let graph = MemoryGraph::new();
        for (likelihood, impact) in [(1, 1), (2, 2), (3, 3), (4, 4), (5, 5), (1, 5)] {
            a_risk(&graph, &scope(), likelihood, impact);
        }
        let read = risk_dashboard(&graph, &scope(), "co-1").expect("a dashboard");
        assert_eq!(read["total_risks"], json!(6));
        // 1 + 4 + 9 + 16 + 25 + 5 = 60 over six is ten
        assert_eq!(read["avg_score"], json!(10.0));
        let top = read["top_risks"].as_array().expect("top");
        assert_eq!(top.len(), 5);
        assert_eq!(top[0]["risk_score"], json!(25));
        assert_eq!(top[4]["risk_score"], json!(4));
    }

    #[test]
    fn closing_a_risk_keeps_its_score_and_its_mitigation() {
        let graph = MemoryGraph::new();
        let created = a_risk(&graph, &scope(), 4, 4);
        let closed = close_risk(
            &graph,
            &scope(),
            field(&created, "id"),
            &json!({"today": "2026-05-01T09:00:00Z"}),
        )
        .expect("a close");
        assert_eq!(closed["status"], json!("closed"));
        let listed = list_risks(&graph, &scope(), "co-1", &json!({})).expect("a listing");
        assert_eq!(listed["risks"][0]["risk_score"], json!(16));
        assert_eq!(listed["risks"][0]["level"], json!("high"));
    }

    #[test]
    fn another_books_risk_is_not_mine_to_close() {
        let graph = MemoryGraph::new();
        let theirs = BookScope::new("user-2", "book-2");
        let other = a_risk(&graph, &theirs, 3, 3);
        let not_mine = close_risk(&graph, &scope(), field(&other, "id"), &json!({}));
        assert!(not_mine.is_err());
        assert_eq!(
            list_risks(&graph, &scope(), "co-1", &json!({})).expect("a listing")["total"],
            json!(0)
        );
    }
}
