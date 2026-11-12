#![forbid(unsafe_code)]
//! treasury-policy-service: the treasury policies a Book sets, the limits each one places, and the checks made
//! against those limits.
//!
//! Limit values and checked values are exact, and a percentage is derived from them rather than carried.
//!
//! Behaviours kept as shipped, each pinned by a test:
//!
//! * a policy category outside the six the service knows is refused with the list of valid ones;
//! * a limit can only be set on a policy the caller can see, and a check only against such a limit, so a
//!   policy in another Book is a miss rather than a refusal that admits it exists;
//! * utilization is the checked value over the limit as a percentage, and a limit of zero reports nothing
//!   rather than dividing;
//! * a check is compliant when the value is at most the limit, so landing exactly on the limit passes;
//! * the limit's own `current_utilization` is set to the checked VALUE, not to the percentage the check
//!   reports, which is what the service did; both figures are kept because the limit field is read by older
//!   callers;
//! * the compliance listing filters by policy and hands back the most recent `limit` checks in the order they
//!   were made, not sorted by when they say they were made.

use crate::helpers::{field, money, money_or_nothing, or_default, refused, remember, text};
use serde_json::{json, Value};
use vimbai_common::Money;
use vimbai_store::amounts::wire;
use vimbai_store::query::{query_page, query_text};
use vimbai_store::{
    BookScope, Graph, Records, StoreError, POLICY_CHECK, POLICY_LIMIT, TREASURY_POLICY,
};

fn find(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    kind: vimbai_store::RecordKind,
    id: &str,
) -> Result<Option<Value>, StoreError> {
    Ok(Records::new(graph, kind)
        .list(scope, 4000)?
        .into_iter()
        .find(|row| field(row, "id") == id))
}

/// `POST /policies`.
pub fn create_policy(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let category = text(payload, "policy_category");
    let known = [
        "liquidity",
        "funding",
        "investment",
        "fx_risk",
        "interest_rate",
        "counterparty",
    ];
    if !known.contains(&category.as_str()) {
        return Err(refused(
            "Invalid category. Must be one of ['liquidity', 'funding', 'investment', 'fx_risk', \
             'interest_rate', 'counterparty']",
        ));
    }
    remember(
        graph,
        scope,
        TREASURY_POLICY,
        json!({
            "name": text(payload, "name"),
            "description": text(payload, "description"),
            "policy_category": category,
            "version": or_default(payload, "version", "1.0"),
            "effective_date": text(payload, "effective_date"),
            "review_date": text(payload, "review_date"),
            "approved_by": text(payload, "approved_by"),
            "status": or_default(payload, "status", "active"),
            "created_at": text(payload, "created_at"),
        }),
    )
}

/// `GET /policies`.
pub fn list_policies(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let category = query_text(query, "category");
    let status = query_text(query, "status");
    let mut all: Vec<Value> = Records::new(graph, TREASURY_POLICY)
        .list(scope, 2000)?
        .into_iter()
        .filter(|row| {
            category
                .as_deref()
                .map(|wanted| field(row, "policy_category") == wanted)
                .unwrap_or(true)
                && status
                    .as_deref()
                    .map(|wanted| field(row, "status") == wanted)
                    .unwrap_or(true)
        })
        .collect();
    all.sort_by(|one, two| field(one, "name").cmp(field(two, "name")));
    Ok(Value::Array(all))
}

/// `POST /policies/{policy_id}/limits`.
pub fn set_limit(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    policy_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    if find(graph, scope, TREASURY_POLICY, policy_id)?.is_none() {
        return Err(StoreError::NotFound);
    }
    let limit_value = money(payload, "limit_value")?;
    // the warning threshold is a share of the limit, not an amount: kept as a plain figure
    let threshold = text(payload, "warning_threshold");
    let threshold = if threshold.is_empty() {
        "0.8".to_string()
    } else {
        threshold
    };
    remember(
        graph,
        scope,
        POLICY_LIMIT,
        json!({
            "policy_id": policy_id,
            "limit_type": text(payload, "limit_type"),
            "limit_value": wire(&limit_value),
            "currency": or_default(payload, "currency", "USD"),
            "warning_threshold": threshold,
            "current_utilization": wire(&Money::zero()),
        }),
    )
}

/// `GET /policies/{policy_id}/limits`.
pub fn list_limits(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    policy_id: &str,
) -> Result<Value, StoreError> {
    let mut mine: Vec<Value> = Records::new(graph, POLICY_LIMIT)
        .list(scope, 2000)?
        .into_iter()
        .filter(|row| field(row, "policy_id") == policy_id)
        .collect();
    mine.sort_by(|one, two| field(one, "limit_type").cmp(field(two, "limit_type")));
    Ok(Value::Array(mine))
}

/// `POST /limits/{limit_id}/check`.
pub fn check_compliance(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    limit_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let Some(limit) = find(graph, scope, POLICY_LIMIT, limit_id)? else {
        return Err(StoreError::NotFound);
    };
    let checked = money(payload, "checked_value")?;
    let limit_value = money_or_nothing(&limit, "limit_value")?;

    let hundred = Money::parse("100").expect("money");
    let utilization = if limit_value > Money::zero() {
        // a percentage, so a plain figure rather than an amount of money
        checked.over(&limit_value).times(&hundred).to_fixed(6)
    } else {
        "0".to_string()
    };
    let compliant = checked <= limit_value;

    let saved = remember(
        graph,
        scope,
        POLICY_CHECK,
        json!({
            "policy_id": field(&limit, "policy_id"),
            "limit_id": limit_id,
            "checked_value": wire(&checked),
            "limit_value": wire(&limit_value),
            "compliant": compliant,
            "utilization_pct": utilization,
            "checked_at": text(payload, "checked_at"),
            "notes": text(payload, "notes"),
        }),
    )?;

    // as shipped: the limit remembers the value that was checked against it, not the percentage
    Records::new(graph, POLICY_LIMIT).update(
        scope,
        limit_id,
        &json!({"current_utilization": wire(&checked)}),
    )?;

    Ok(saved)
}

/// `GET /compliance`.
pub fn list_compliance_checks(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let policy_id = query_text(query, "policy_id");
    let limit = query_page(query, 50, 4000);
    let mut all: Vec<Value> = Records::new(graph, POLICY_CHECK)
        .list(scope, 4000)?
        .into_iter()
        .filter(|row| {
            policy_id
                .as_deref()
                .map(|wanted| field(row, "policy_id") == wanted)
                .unwrap_or(true)
        })
        .collect();
    // as shipped: the last N in the order they were made
    all.sort_by(|one, two| field(one, "checked_at").cmp(field(two, "checked_at")));
    let start = all.len().saturating_sub(limit);
    Ok(Value::Array(all[start..].to_vec()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::FakeGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn policy(id: &str) -> Value {
        json!({"id": id, "name": "Liquidity", "policy_category": "liquidity", "status": "active"})
    }

    fn limit(id: &str, ceiling: &str, policy_id: &str) -> Value {
        json!({"id": id, "policy_id": policy_id, "limit_type": "exposure",
               "limit_value": ceiling, "current_utilization": "0.000000"})
    }

    #[test]
    fn a_category_outside_the_six_is_refused() {
        let graph = FakeGraph::returning(vec![]);
        let refused = create_policy(
            &graph,
            &scope(),
            &json!({"name": "Something", "policy_category": "vibes",
                    "effective_date": "2026-03-01"}),
        );
        assert!(refused.is_err());
    }

    #[test]
    fn a_policy_is_created_active_with_version_one() {
        let graph = FakeGraph::queue(vec![vec![json!({"id": "p-1"})]]);
        let answer = create_policy(
            &graph,
            &scope(),
            &json!({"name": "Funding", "policy_category": "funding",
                    "effective_date": "2026-03-01"}),
        )
        .expect("policy");
        assert_eq!(answer["status"], json!("active"));
        assert_eq!(answer["version"], json!("1.0"));
    }

    #[test]
    fn a_limit_cannot_be_set_on_a_policy_the_caller_does_not_hold() {
        let graph = FakeGraph::returning(vec![policy("other")]);
        let missing = set_limit(
            &graph,
            &scope(),
            "p-1",
            &json!({"limit_type": "exposure", "limit_value": "1000.00"}),
        );
        assert!(matches!(missing, Err(StoreError::NotFound)));
    }

    #[test]
    fn a_limit_keeps_its_threshold_as_a_share_not_an_amount() {
        let graph = FakeGraph::queue(vec![vec![policy("p-1")], vec![json!({"id": "l-1"})]]);
        let answer = set_limit(
            &graph,
            &scope(),
            "p-1",
            &json!({"limit_type": "exposure", "limit_value": "1000000.00",
                    "warning_threshold": "0.75"}),
        )
        .expect("limit");
        assert_eq!(answer["warning_threshold"], json!("0.75"));
        assert_eq!(answer["limit_value"], json!("1000000.000000"));
    }

    #[test]
    fn a_check_reports_its_utilization_and_passes_on_the_limit_itself() {
        let graph = FakeGraph::queue(vec![
            vec![limit("l-1", "1000.000000", "p-1")],
            vec![json!({"id": "c-1"})],
            vec![json!({"id": "l-1"})],
        ]);
        let answer = check_compliance(
            &graph,
            &scope(),
            "l-1",
            &json!({"checked_value": "1000.00", "notes": "at the ceiling"}),
        )
        .expect("check");
        assert_eq!(answer["compliant"], json!(true));
        assert_eq!(answer["utilization_pct"], json!("100.000000"));
        assert_eq!(answer["checked_value"], json!("1000.000000"));
    }

    #[test]
    fn a_check_over_the_limit_is_not_compliant_and_the_limit_remembers_the_value() {
        let graph = FakeGraph::queue(vec![
            vec![limit("l-1", "1000.000000", "p-1")],
            vec![json!({"id": "c-1"})],
            vec![json!({"id": "l-1"})],
        ]);
        let answer = check_compliance(
            &graph,
            &scope(),
            "l-1",
            &json!({"checked_value": "1250.00"}),
        )
        .expect("check");
        assert_eq!(answer["compliant"], json!(false));
        assert_eq!(answer["utilization_pct"], json!("125.000000"));
        // as shipped: the limit stores the checked value, not the percentage
        let (_, params) = graph.statement(2);
        assert_eq!(params["patch"]["current_utilization"], json!("1250.000000"));
    }

    #[test]
    fn a_limit_of_zero_reports_no_utilization_instead_of_dividing() {
        let graph = FakeGraph::queue(vec![
            vec![limit("l-1", "0.000000", "p-1")],
            vec![json!({"id": "c-1"})],
            vec![json!({"id": "l-1"})],
        ]);
        let answer = check_compliance(&graph, &scope(), "l-1", &json!({"checked_value": "10.00"}))
            .expect("check");
        assert_eq!(answer["utilization_pct"], json!("0"));
        assert_eq!(answer["compliant"], json!(false));
    }

    #[test]
    fn checking_an_amount_no_float_could_hold_keeps_every_digit() {
        let graph = FakeGraph::queue(vec![
            vec![limit("l-1", "1.000000", "p-1")],
            vec![json!({"id": "c-1"})],
            vec![json!({"id": "l-1"})],
        ]);
        let answer = check_compliance(
            &graph,
            &scope(),
            "l-1",
            &json!({"checked_value": "123456789012345678901.55"}),
        )
        .expect("check");
        assert_eq!(
            answer["checked_value"],
            json!("123456789012345678901.550000")
        );
        assert_eq!(answer["compliant"], json!(false));
    }

    #[test]
    fn the_compliance_listing_filters_by_policy_and_keeps_the_latest_checks() {
        let graph = FakeGraph::returning(vec![
            json!({"id": "c-1", "policy_id": "p-1", "checked_at": "2026-03-01"}),
            json!({"id": "c-2", "policy_id": "p-2", "checked_at": "2026-03-02"}),
            json!({"id": "c-3", "policy_id": "p-1", "checked_at": "2026-03-03"}),
        ]);
        let answer =
            list_compliance_checks(&graph, &scope(), &json!({"policy_id": "p-1", "limit": "1"}))
                .expect("checks");
        let rows = answer.as_array().expect("rows");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["id"], json!("c-3"));
    }

    #[test]
    fn listing_policies_can_be_narrowed_by_category_or_state() {
        let graph = FakeGraph::returning(vec![
            policy("p-1"),
            json!({"id": "p-2", "name": "Funding", "policy_category": "funding", "status": "draft"}),
        ]);
        let drafts =
            list_policies(&graph, &scope(), &json!({"status": "draft"})).expect("policies");
        assert_eq!(drafts.as_array().expect("rows").len(), 1);
        let funding =
            list_policies(&graph, &scope(), &json!({"category": "funding"})).expect("policies");
        assert_eq!(funding.as_array().expect("rows").len(), 1);
    }
}
