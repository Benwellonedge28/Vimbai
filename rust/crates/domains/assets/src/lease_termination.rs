#![forbid(unsafe_code)]
//! lease-termination-service's records, on the store.
//!
//! Ending a lease early costs what the remaining payments would have been, plus any penalty, less
//! whatever the landlord gives back for the asset. The settlement is the figure two parties sign, so
//! it is exact: a settlement assembled from doubles is a number nobody can agree on.
//!
//! Kept as shipped:
//!
//! * Opening a termination records a settlement figure of remaining payments times the payment
//!   amount plus the penalty, before anything is given back. That figure is written once, at
//!   creation, and is not updated when a settlement is later calculated, so the two can differ.
//! * A settlement may be calculated more than once; every calculation is kept.
//! * Approval is only possible from `pending` and completion only from `approved`, in the service's
//!   own words: "Termination is not pending" and "Termination must be approved first".
//! * Statuses are `pending`, `approved`, `completed` and `rejected`, and only the first three are
//!   reachable through a door. `rejected` is carried in the record and never set by one.

use serde_json::{json, Value};
use vimbai_common::Money;

use vimbai_store::amounts::{amount, amount_or_zero, stored_amount, wire};
use vimbai_store::{
    BookScope, Graph, Records, StoreError, LEASE_TERMINATION, TERMINATION_SETTLEMENT,
};

fn text(payload: &Value, key: &str) -> String {
    payload
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn required(payload: &Value, key: &str) -> Result<String, StoreError> {
    let value = text(payload, key);
    if value.trim().is_empty() {
        return Err(StoreError::Invalid(format!("{key} is required")));
    }
    Ok(value)
}

fn payments_remaining(payload: &Value) -> Result<i64, StoreError> {
    match payload.get("remaining_payments") {
        Some(Value::Number(number)) => number
            .as_i64()
            .ok_or_else(|| StoreError::Invalid("remaining_payments must be a whole number".into())),
        Some(Value::String(text)) => text
            .trim()
            .parse::<i64>()
            .map_err(|_| StoreError::Invalid("remaining_payments must be a whole number".into())),
        _ => Err(StoreError::Invalid("remaining_payments is required".into())),
    }
}

/// `/terminations`: open a lease exit.
pub fn create_termination<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let remaining = payments_remaining(payload)?;
    let payment = amount(payload, "remaining_payment_amount")?;
    let penalty = amount_or_zero(payload, "early_termination_penalty")?;

    // the figure recorded at opening: every payment still due plus the penalty, before anything is
    // given back for the asset
    let settlement = payment.times(&Money::from_i64(remaining)).plus(&penalty);

    Records::new(graph, LEASE_TERMINATION).create(
        scope,
        &json!({
            "lease_id": required(payload, "lease_id")?,
            "termination_date": required(payload, "termination_date")?,
            "original_end_date": required(payload, "original_end_date")?,
            "remaining_payments": remaining,
            "remaining_payment_amount": wire(&payment),
            "early_termination_penalty": wire(&penalty),
            "settlement_amount": wire(&settlement),
            "reason": text(payload, "reason"),
            "status": "pending",
            "approved_by": "",
        }),
    )
}

/// `/terminations`: the caller's exits, optionally narrowed by status.
pub fn list_terminations<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    status: Option<&str>,
) -> Result<Value, StoreError> {
    let rows = Records::new(graph, LEASE_TERMINATION).list(scope, 1000)?;
    Ok(Value::Array(
        rows.into_iter()
            .filter(|row| match status {
                None => true,
                Some(wanted) => row.get("status").and_then(Value::as_str) == Some(wanted),
            })
            .collect(),
    ))
}

pub fn get_termination<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    termination_id: &str,
) -> Result<Value, StoreError> {
    Records::new(graph, LEASE_TERMINATION).get(scope, termination_id)
}

/// `/terminations/{id}/settlement`: what ending the lease now actually costs.
pub fn calculate_settlement<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    termination_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let termination = get_termination(graph, scope, termination_id)?;

    let remaining = termination
        .get("remaining_payments")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let payment = stored_amount(&termination, "remaining_payment_amount")?;
    let penalty = stored_amount(&termination, "early_termination_penalty")?;
    let returned = amount_or_zero(payload, "asset_return_value")?;

    let remaining_value = payment.times(&Money::from_i64(remaining));
    let net = remaining_value.plus(&penalty).minus(&returned);

    let fields = json!({
        "termination_id": termination_id,
        "remaining_lease_payments": wire(&remaining_value),
        "early_termination_penalty": wire(&penalty),
        "asset_return_value": wire(&returned),
        "net_settlement": wire(&net),
    });
    let saved = Records::new(graph, TERMINATION_SETTLEMENT).create(scope, &fields)?;

    let mut answer = fields.clone();
    if let Some(id) = saved.get("id") {
        answer["id"] = id.clone();
    }
    Ok(answer)
}

/// `/terminations/{id}/approve`: only a pending exit can be approved.
pub fn approve<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    termination_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let termination = get_termination(graph, scope, termination_id)?;
    let status = termination
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("pending");
    if status != "pending" {
        return Err(StoreError::Invalid("Termination is not pending".into()));
    }
    let approved_by = text(payload, "approved_by");

    Records::new(graph, LEASE_TERMINATION).update(
        scope,
        termination_id,
        &json!({"status": "approved", "approved_by": approved_by}),
    )?;
    Ok(json!({"termination_id": termination_id, "status": "approved"}))
}

/// `/terminations/{id}/complete`: only an approved exit can be completed.
pub fn complete<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    termination_id: &str,
) -> Result<Value, StoreError> {
    let termination = get_termination(graph, scope, termination_id)?;
    let status = termination
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("pending");
    if status != "approved" {
        return Err(StoreError::Invalid(
            "Termination must be approved first".into(),
        ));
    }

    Records::new(graph, LEASE_TERMINATION).update(
        scope,
        termination_id,
        &json!({"status": "completed", "completed_at": "now"}),
    )?;
    Ok(json!({"termination_id": termination_id, "status": "completed"}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::FakeGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn stored(fields: Value) -> Value {
        let mut merged = json!({
            "id": "t-1",
            "lease_id": "L-1",
            "remaining_payments": 6,
            "remaining_payment_amount": "2500",
            "early_termination_penalty": "1000",
            "settlement_amount": "16000",
            "status": "pending"
        })
        .as_object()
        .unwrap()
        .clone();
        for (key, value) in fields.as_object().unwrap() {
            merged.insert(key.clone(), value.clone());
        }
        Value::Object(merged)
    }

    #[test]
    fn opening_records_every_payment_still_due_plus_the_penalty() {
        let graph = FakeGraph::returning(vec![json!({"id": "t-1"})]);
        create_termination(
            &graph,
            &scope(),
            &json!({"lease_id": "L-1", "termination_date": "2026-12-31",
                    "original_end_date": "2027-06-30", "remaining_payments": 6,
                    "remaining_payment_amount": "2500", "early_termination_penalty": "1000"}),
        )
        .expect("opened");
        let (_, params) = graph.last();
        assert_eq!(params["props"]["settlement_amount"], json!("16000.000000"));
        assert_eq!(params["props"]["status"], json!("pending"));
    }

    #[test]
    fn the_settlement_gives_back_the_asset_and_stays_exact() {
        let graph = FakeGraph::returning(vec![stored(json!({
            "remaining_payment_amount": "123456789012345678901.55",
            "remaining_payments": 2
        }))]);
        let answer = calculate_settlement(
            &graph,
            &scope(),
            "t-1",
            &json!({"asset_return_value": "0.45"}),
        )
        .expect("settled");
        // 2 x 123456789012345678901.55, plus 1000, less 0.45
        assert_eq!(
            answer["remaining_lease_payments"],
            json!("246913578024691357803.100000")
        );
        assert_eq!(
            answer["net_settlement"],
            json!("246913578024691358802.650000")
        );
    }

    #[test]
    fn a_settlement_with_nothing_given_back_costs_the_full_run() {
        let graph = FakeGraph::returning(vec![stored(json!({}))]);
        let answer = calculate_settlement(&graph, &scope(), "t-1", &json!({})).expect("settled");
        // 6 x 2500 plus the 1000 penalty
        assert_eq!(answer["net_settlement"], json!("16000.000000"));
    }

    #[test]
    fn only_a_pending_exit_can_be_approved() {
        let graph = FakeGraph::returning(vec![stored(json!({"status": "approved"}))]);
        let error =
            approve(&graph, &scope(), "t-1", &json!({"approved_by": "sam"})).expect_err("refused");
        assert_eq!(error.to_string(), "Termination is not pending");
    }

    #[test]
    fn only_an_approved_exit_can_be_completed() {
        let graph = FakeGraph::returning(vec![stored(json!({"status": "pending"}))]);
        let error = complete(&graph, &scope(), "t-1").expect_err("refused");
        assert_eq!(error.to_string(), "Termination must be approved first");

        let approved = FakeGraph::returning(vec![stored(json!({"status": "approved"}))]);
        let answer = complete(&approved, &scope(), "t-1").expect("completed");
        assert_eq!(answer["status"], json!("completed"));
    }
}
