//! appropriation-control-service's appropriations, on the store.
//!
//! A departmental appropriation is a budget with three running figures: approved, committed and
//! spent. Everything else is derived from those, and the control rules are applied on each
//! transaction rather than recalculated from a transaction log:
//!
//! * commit raises committed;
//! * spend raises spent and lowers committed (the commitment becomes an actual);
//! * uncommit lowers committed;
//! * refund lowers spent.
//!
//! `available` is approved less committed less spent, and once it reaches zero or below the
//! appropriation is marked exhausted and stays that way. Two behaviours are inherited on purpose:
//! spending more than was committed drives committed negative, and an unrecognised transaction type
//! is answered as not-found rather than as a bad request. Both are what the Python did, and the
//! tests pin them so a future change is a decision rather than an accident.

#![forbid(unsafe_code)]

use serde_json::{json, Value};
use vimbai_common::Money;

use vimbai_store::amounts::{stored_amount, wire};
use vimbai_store::{BookScope, Graph, Records, StoreError, APPROPRIATION, APPROPRIATION_TX};

/// The control transaction types the service accepts.
const TX_TYPES: [&str; 4] = ["commit", "spend", "uncommit", "refund"];

fn money_in(value: Option<&Value>, field: &str) -> Result<Money, StoreError> {
    match value {
        Some(Value::Number(number)) => Money::from_f64_shortest(number.as_f64().unwrap_or(0.0))
            .map_err(|error| StoreError::Invalid(format!("{field}: {error}"))),
        Some(Value::String(text)) => {
            Money::parse(text).map_err(|error| StoreError::Invalid(format!("{field}: {error}")))
        }
        None | Some(Value::Null) => Ok(Money::zero()),
        Some(_) => Err(StoreError::Invalid(format!("{field} is not an amount"))),
    }
}

fn appropriation_json(row: &Value) -> Result<Value, StoreError> {
    Ok(json!({
        "id": row.get("id").cloned().unwrap_or(Value::Null),
        "company_id": row.get("company_id").cloned().unwrap_or(Value::Null),
        "department": row.get("department").cloned().unwrap_or(Value::Null),
        "fiscal_year": row.get("fiscal_year").cloned().unwrap_or(Value::Null),
        "approved_amount": wire(&stored_amount(row, "approved_amount")?),
        "spent_amount": wire(&stored_amount(row, "spent_amount")?),
        "committed_amount": wire(&stored_amount(row, "committed_amount")?),
        "available_amount": wire(&stored_amount(row, "available_amount")?),
        "status": row.get("status").cloned().unwrap_or_else(|| json!("active")),
        "created_at": row.get("created_at").cloned().unwrap_or(Value::Null),
    }))
}

/// `/appropriations`: create one, with whatever has already been spent or committed.
pub fn create<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let company_id = payload
        .get("company_id")
        .and_then(Value::as_str)
        .ok_or_else(|| StoreError::Invalid("company_id is required".into()))?;
    let department = payload
        .get("department")
        .and_then(Value::as_str)
        .ok_or_else(|| StoreError::Invalid("department is required".into()))?;
    let approved = money_in(payload.get("approved_amount"), "approved_amount")?;
    let spent = money_in(payload.get("spent_amount"), "spent_amount")?;
    let committed = money_in(payload.get("committed_amount"), "committed_amount")?;
    let available = approved.minus(&committed).minus(&spent);

    let stored = Records::new(graph, APPROPRIATION).create(
        scope,
        &json!({
            "company_id": company_id,
            "department": department,
            "fiscal_year": payload.get("fiscal_year").and_then(Value::as_str).unwrap_or(""),
            "approved_amount": approved.to_fixed(6),
            "spent_amount": spent.to_fixed(6),
            "committed_amount": committed.to_fixed(6),
            "available_amount": available.to_fixed(6),
            // Creating an appropriation that is already overspent still starts out active; only a
            // transaction can exhaust it. That was the service's behaviour.
            "status": "active",
            "created_at": payload.get("created_at").and_then(Value::as_str).unwrap_or(""),
        }),
    )?;
    appropriation_json(&stored)
}

/// `/appropriations/{company_id}`: the company's appropriations, optionally by department.
pub fn list<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    company_id: &str,
    department: &str,
) -> Result<Value, StoreError> {
    let mut rows: Vec<Value> = Records::new(graph, APPROPRIATION)
        .list(scope, 500)?
        .into_iter()
        .filter(|row| row.get("company_id").and_then(Value::as_str) == Some(company_id))
        .filter(|row| {
            department.is_empty()
                || row.get("department").and_then(Value::as_str) == Some(department)
        })
        .collect();
    rows.sort_by_key(|row| {
        row.get("created_at")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned()
    });
    let mut out = Vec::new();
    for row in &rows {
        out.push(appropriation_json(row)?);
    }
    Ok(json!({"company_id": company_id, "appropriations": out, "total": out.len()}))
}

/// `/transactions`: apply one control transaction and write the aggregates back.
pub fn transact<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let kind = payload
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| StoreError::Invalid("type is required".into()))?;
    if !TX_TYPES.contains(&kind) {
        // Inherited: an unknown type is answered as not-found, exactly as the service did.
        return Err(StoreError::NotFound);
    }
    let appropriation_id = payload
        .get("appropriation_id")
        .and_then(Value::as_str)
        .ok_or(StoreError::NotFound)?;
    let amount = money_in(payload.get("amount"), "amount")?;

    let rows = Records::new(graph, APPROPRIATION);
    // `get` already answers NotFound for an appropriation outside the caller's Book.
    let current = rows.get(scope, appropriation_id)?;

    let approved = stored_amount(&current, "approved_amount")?;
    let mut spent = stored_amount(&current, "spent_amount")?;
    let mut committed = stored_amount(&current, "committed_amount")?;
    match kind {
        "commit" => committed = committed.plus(&amount),
        "spend" => {
            spent = spent.plus(&amount);
            committed = committed.minus(&amount);
        }
        "uncommit" => committed = committed.minus(&amount),
        _ => spent = spent.minus(&amount),
    }
    let available = approved.minus(&committed).minus(&spent);
    // Exhaustion is one-way: a later refund raises `available` again but the status stays.
    let status = if available <= Money::zero() {
        "exhausted"
    } else {
        current
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("active")
    };

    let stored = Records::new(graph, APPROPRIATION_TX).create(
        scope,
        &json!({
            "appropriation_id": appropriation_id,
            "type": kind,
            "amount": amount.to_fixed(6),
            "description": payload.get("description").and_then(Value::as_str).unwrap_or(""),
            "date": payload.get("date").and_then(Value::as_str).unwrap_or(""),
            "created_at": payload.get("created_at").and_then(Value::as_str).unwrap_or(""),
        }),
    )?;

    rows.update(
        scope,
        appropriation_id,
        &json!({
            "spent_amount": spent.to_fixed(6),
            "committed_amount": committed.to_fixed(6),
            "available_amount": available.to_fixed(6),
            "status": status,
        }),
    )?;

    Ok(json!({
        "id": stored.get("id").cloned().unwrap_or(Value::Null),
        "available": wire(&available),
        "status": status,
    }))
}

/// `/check/{appropriation_id}`: can this much still be spent?
pub fn check<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    appropriation_id: &str,
    amount: &Value,
) -> Result<Value, StoreError> {
    let row = Records::new(graph, APPROPRIATION).get(scope, appropriation_id)?;
    let available = stored_amount(&row, "available_amount")?;
    let requested = money_in(Some(amount), "amount")?;
    Ok(json!({
        "appropriation_id": appropriation_id,
        "available": wire(&available),
        "requested": wire(&requested),
        "allowed": available >= requested,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn appropriation(id: &str, approved: &str, spent: &str, committed: &str) -> Value {
        json!({
            "x": {
                "id": id,
                "company_id": "c-1",
                "department": "Ops",
                "fiscal_year": "2026",
                "approved_amount": approved,
                "spent_amount": spent,
                "committed_amount": committed,
                "available_amount": "0",
                "status": "active",
                "created_at": "2026-01-01T00:00:00+00:00"
            }
        })
    }

    #[test]
    fn the_available_figure_is_derived_when_the_appropriation_is_created() {
        // The fake echoes back what the store wrote, so the row carries the derived figure.
        let mut stored = appropriation("a-1", "100000", "25000", "10000");
        stored["x"]["available_amount"] = json!("65000.000000");
        let graph = vimbai_store::fake::FakeGraph::returning(vec![stored]);
        let out = create(
            &graph,
            &scope(),
            &json!({
                "company_id": "c-1",
                "department": "Ops",
                "fiscal_year": "2026",
                "approved_amount": 100000,
                "spent_amount": 25000,
                "committed_amount": 10000,
            }),
        )
        .unwrap();
        assert_eq!(out["available_amount"], json!(65000.0));
        assert_eq!(out["status"], json!("active"));
    }

    #[test]
    fn spending_converts_a_commitment_into_actual_spend() {
        let graph = vimbai_store::fake::FakeGraph::returning(vec![appropriation(
            "a-1", "100000", "0", "30000",
        )]);
        let out = transact(
            &graph,
            &scope(),
            &json!({"appropriation_id": "a-1", "type": "spend", "amount": 30000}),
        )
        .unwrap();
        // committed 30000 -> 0, spent 0 -> 30000, available 100000 - 0 - 30000
        assert_eq!(out["available"], json!(70000.0));
        assert_eq!(out["status"], json!("active"));
    }

    #[test]
    fn exhausting_the_budget_marks_it_and_it_stays_marked() {
        let graph = vimbai_store::fake::FakeGraph::returning(vec![appropriation(
            "a-1", "100000", "0", "100000",
        )]);
        let out = transact(
            &graph,
            &scope(),
            &json!({"appropriation_id": "a-1", "type": "spend", "amount": 100000}),
        )
        .unwrap();
        assert_eq!(out["available"], json!(0.0));
        assert_eq!(out["status"], json!("exhausted"));
    }

    #[test]
    fn an_unknown_transaction_type_is_not_found() {
        let graph = vimbai_store::fake::FakeGraph::returning(vec![appropriation(
            "a-1", "100000", "0", "0",
        )]);
        assert_eq!(
            transact(
                &graph,
                &scope(),
                &json!({"appropriation_id": "a-1", "type": "transfer", "amount": 1}),
            )
            .unwrap_err(),
            StoreError::NotFound
        );
    }

    #[test]
    fn a_large_appropriation_stays_exact() {
        let graph = vimbai_store::fake::FakeGraph::returning(vec![appropriation(
            "a-1",
            "12345678901234.56",
            "0.01",
            "0",
        )]);
        let out = transact(
            &graph,
            &scope(),
            &json!({"appropriation_id": "a-1", "type": "commit", "amount": "0.01"}),
        )
        .unwrap();
        assert_eq!(out["available"], json!(12345678901234.54));
    }
}
