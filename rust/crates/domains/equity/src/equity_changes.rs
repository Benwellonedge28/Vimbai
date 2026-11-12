//! equity-changes-service's records, on the store.
//!
//! Two kinds of record: the individual equity transactions, and the statement that rolls them up
//! by type (issuances, buybacks, dividends, retained earnings, other) into an ending equity. The
//! roll-up is computed here rather than accepted, and it runs on `Money`, so a share issue worth
//! twelve figures does not drift while it is being added.
//!
//! One behaviour is deliberate and inherited: a transaction with no amount set but a share count
//! and a price has its amount derived from `shares * price_per_share`. The service did that on
//! both the create and the statement path, and callers rely on it.

#![forbid(unsafe_code)]

use serde_json::{json, Value};
use vimbai_common::Money;

use vimbai_store::amounts::{stored_amount, wire};
use vimbai_store::{BookScope, Graph, Records, StoreError, EQUITY_STATEMENT, EQUITY_TRANSACTION};

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

/// The amount a transaction carries: what it says, or shares times price when it says nothing.
fn transaction_amount(transaction: &Value) -> Result<Money, StoreError> {
    let amount = money_in(transaction.get("amount"), "amount")?;
    let shares = transaction
        .get("shares")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    if amount.is_zero() && shares > 0 {
        let price = money_in(transaction.get("price_per_share"), "price_per_share")?;
        return Ok(price.times(&Money::from_i64(shares)));
    }
    Ok(amount)
}

/// A transaction in the shape the service published.
fn transaction_of(transaction: &Value, amount: &Money) -> Value {
    json!({
        "id": transaction.get("id").cloned().unwrap_or(Value::Null),
        "company_id": transaction.get("company_id").cloned().unwrap_or(Value::Null),
        "transaction_type": transaction.get("transaction_type").cloned().unwrap_or(Value::Null),
        "shareholder": transaction.get("shareholder").and_then(Value::as_str).unwrap_or(""),
        "shares": transaction.get("shares").and_then(Value::as_i64).unwrap_or(0),
        "price_per_share": wire(&money_in(transaction.get("price_per_share"), "price_per_share").unwrap_or_else(|_| Money::zero())),
        "amount": wire(amount),
        "description": transaction.get("description").and_then(Value::as_str).unwrap_or(""),
        "date": transaction.get("date").cloned().unwrap_or(Value::Null),
        "created_at": transaction.get("created_at").cloned().unwrap_or(Value::Null),
    })
}

/// `/transactions`: record one equity transaction.
pub fn create_transaction<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let company_id = payload
        .get("company_id")
        .and_then(Value::as_str)
        .ok_or_else(|| StoreError::Invalid("company_id is required".into()))?;
    let transaction_type = payload
        .get("transaction_type")
        .and_then(Value::as_str)
        .ok_or_else(|| StoreError::Invalid("transaction_type is required".into()))?;
    let amount = transaction_amount(payload)?;

    let record = json!({
        "company_id": company_id,
        "transaction_type": transaction_type,
        "shareholder": payload.get("shareholder").and_then(Value::as_str).unwrap_or(""),
        "shares": payload.get("shares").and_then(Value::as_i64).unwrap_or(0),
        "price_per_share": money_in(payload.get("price_per_share"), "price_per_share")?.to_fixed(6),
        "amount": amount.to_fixed(6),
        "description": payload.get("description").and_then(Value::as_str).unwrap_or(""),
        "date": payload.get("date").and_then(Value::as_str).unwrap_or(""),
        "created_at": payload.get("created_at").and_then(Value::as_str).unwrap_or(""),
    });
    let stored = Records::new(graph, EQUITY_TRANSACTION).create(scope, &record)?;
    Ok(transaction_of(&stored, &amount))
}

/// `/transactions/{company_id}`: the caller's transactions, optionally by type.
pub fn transactions<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    company_id: &str,
    transaction_type: Option<&str>,
) -> Result<Value, StoreError> {
    let rows = Records::new(graph, EQUITY_TRANSACTION).list(scope, 500)?;
    let mut out = Vec::new();
    for row in rows {
        if row.get("company_id").and_then(Value::as_str) != Some(company_id) {
            continue;
        }
        if let Some(wanted) = transaction_type {
            if row.get("transaction_type").and_then(Value::as_str) != Some(wanted) {
                continue;
            }
        }
        let amount = stored_amount(&row, "amount")?;
        out.push(transaction_of(&row, &amount));
    }
    Ok(json!({"company_id": company_id, "transactions": out, "total": out.len()}))
}

/// `/statement`: roll the transactions up by type and store the statement.
pub fn generate_statement<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let company_id = payload
        .get("company_id")
        .and_then(Value::as_str)
        .ok_or_else(|| StoreError::Invalid("company_id is required".into()))?;
    let period = payload
        .get("period")
        .and_then(Value::as_str)
        .ok_or_else(|| StoreError::Invalid("period is required".into()))?;
    let beginning_equity = money_in(payload.get("beginning_equity"), "beginning_equity")?;

    let mut issuances = Money::zero();
    let mut buybacks = Money::zero();
    let mut dividends = Money::zero();
    let mut retained = Money::zero();
    let mut other = Money::zero();
    let mut lines = Vec::new();

    for transaction in payload
        .get("transactions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
    {
        let amount = transaction_amount(&transaction)?;
        match transaction
            .get("transaction_type")
            .and_then(Value::as_str)
            .unwrap_or("")
        {
            "issuance" => issuances = issuances.plus(&amount),
            "buyback" => buybacks = buybacks.plus(&amount),
            "dividend" => dividends = dividends.plus(&amount),
            "retained_earnings" => retained = retained.plus(&amount),
            "split" | "transfer" => other = other.plus(&amount),
            _ => {}
        }
        let mut stored = transaction.clone();
        stored["amount"] = json!(amount.to_fixed(6));
        stored["company_id"] = json!(company_id);
        lines.push(stored);
    }

    let ending_equity = beginning_equity
        .plus(&issuances)
        .minus(&buybacks)
        .minus(&dividends)
        .plus(&retained)
        .plus(&other);

    let record = json!({
        "company_id": company_id,
        "period": period,
        "beginning_equity": beginning_equity.to_fixed(6),
        "share_issuances": issuances.to_fixed(6),
        "share_buybacks": buybacks.to_fixed(6),
        "dividends_paid": dividends.to_fixed(6),
        "retained_earnings_change": retained.to_fixed(6),
        "other_changes": other.to_fixed(6),
        "ending_equity": ending_equity.to_fixed(6),
        "transactions": serde_json::to_string(&lines).unwrap_or_else(|_| "[]".to_owned()),
        "created_at": payload.get("created_at").and_then(Value::as_str).unwrap_or(""),
    });
    let stored = Records::new(graph, EQUITY_STATEMENT).create(scope, &record)?;
    let transactions: Vec<Value> = lines
        .iter()
        .map(|line| {
            let amount = money_in(line.get("amount"), "amount").unwrap_or_else(|_| Money::zero());
            transaction_of(line, &amount)
        })
        .collect();

    Ok(json!({
        "id": stored.get("id").cloned().unwrap_or(Value::Null),
        "company_id": company_id,
        "period": period,
        "beginning_equity": wire(&beginning_equity),
        "share_issuances": wire(&issuances),
        "share_buybacks": wire(&buybacks),
        "dividends_paid": wire(&dividends),
        "retained_earnings_change": wire(&retained),
        "other_changes": wire(&other),
        "ending_equity": wire(&ending_equity),
        "transactions": transactions,
        "created_at": record["created_at"],
    }))
}

/// A stored statement, read back from the graph.
fn statement_of(record: &Value) -> Result<Value, StoreError> {
    let lines: Vec<Value> = record
        .get("transactions")
        .and_then(Value::as_str)
        .and_then(|text| serde_json::from_str(text).ok())
        .unwrap_or_default();
    let transactions: Vec<Value> = lines
        .iter()
        .map(|line| {
            let amount = money_in(line.get("amount"), "amount").unwrap_or_else(|_| Money::zero());
            transaction_of(line, &amount)
        })
        .collect();
    let text = |key: &str| record.get(key).cloned().unwrap_or(Value::Null);
    Ok(json!({
        "id": text("id"),
        "company_id": text("company_id"),
        "period": text("period"),
        "beginning_equity": wire(&stored_amount(record, "beginning_equity")?),
        "share_issuances": wire(&stored_amount(record, "share_issuances")?),
        "share_buybacks": wire(&stored_amount(record, "share_buybacks")?),
        "dividends_paid": wire(&stored_amount(record, "dividends_paid")?),
        "retained_earnings_change": wire(&stored_amount(record, "retained_earnings_change")?),
        "other_changes": wire(&stored_amount(record, "other_changes")?),
        "ending_equity": wire(&stored_amount(record, "ending_equity")?),
        "transactions": transactions,
        "created_at": text("created_at"),
    }))
}

/// `/statements/{company_id}`: every statement for the company, with the count.
pub fn statements<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    company_id: &str,
) -> Result<Value, StoreError> {
    let rows = Records::new(graph, EQUITY_STATEMENT).list(scope, 500)?;
    let mut out = Vec::new();
    for row in rows {
        if row.get("company_id").and_then(Value::as_str) == Some(company_id) {
            out.push(statement_of(&row)?);
        }
    }
    Ok(json!({"company_id": company_id, "statements": out, "total": out.len()}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::FakeGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    #[test]
    fn an_unpriced_transaction_derives_its_amount_from_shares_and_price() {
        let graph = FakeGraph::returning(vec![json!({"x": {"id": "tx-1"}})]);
        let out = create_transaction(
            &graph,
            &scope(),
            &json!({
                "company_id": "c-1",
                "transaction_type": "issuance",
                "shares": 2000,
                "price_per_share": "2.5",
            }),
        )
        .unwrap();
        assert_eq!(out["amount"], json!("5000.000000"));
        let (_, params) = graph.last();
        assert_eq!(params["props"]["amount"], json!("5000.000000"));
    }

    #[test]
    fn the_statement_rolls_up_by_type_and_lands_on_the_ending_equity() {
        let graph = FakeGraph::returning(vec![json!({"x": {"id": "stmt-1"}})]);
        let out = generate_statement(
            &graph,
            &scope(),
            &json!({
                "company_id": "c-1",
                "period": "2026",
                "beginning_equity": 100000,
                "transactions": [
                    {"transaction_type": "issuance", "shares": 1000, "price_per_share": 10},
                    {"transaction_type": "buyback", "amount": 5000},
                    {"transaction_type": "dividend", "amount": 2000},
                    {"transaction_type": "retained_earnings", "amount": 750},
                    {"transaction_type": "transfer", "amount": 250}
                ],
            }),
        )
        .unwrap();
        assert_eq!(out["share_issuances"], json!("10000.000000"));
        assert_eq!(out["share_buybacks"], json!("5000.000000"));
        assert_eq!(out["dividends_paid"], json!("2000.000000"));
        assert_eq!(out["retained_earnings_change"], json!("750.000000"));
        assert_eq!(out["other_changes"], json!("250.000000"));
        // 100000 + 10000 - 5000 - 2000 + 750 + 250
        assert_eq!(out["ending_equity"], json!("104000.000000"));
    }

    #[test]
    fn a_long_issue_stays_exact() {
        let graph = FakeGraph::returning(vec![json!({"x": {"id": "stmt-2"}})]);
        let out = generate_statement(
            &graph,
            &scope(),
            &json!({
                "company_id": "c-1",
                "period": "2026",
                "beginning_equity": "0.01",
                "transactions": [
                    {"transaction_type": "issuance", "amount": "12345678901234.56"}
                ],
            }),
        )
        .unwrap();
        assert_eq!(out["ending_equity"], json!("12345678901234.570000"));
    }

    #[test]
    fn listing_filters_by_company_and_type() {
        let graph = FakeGraph::returning(vec![
            json!({"x": {"id": "tx-1", "company_id": "c-1", "transaction_type": "issuance", "amount": "10"}}),
            json!({"x": {"id": "tx-2", "company_id": "c-1", "transaction_type": "dividend", "amount": "5"}}),
            json!({"x": {"id": "tx-3", "company_id": "c-2", "transaction_type": "issuance", "amount": "7"}}),
        ]);
        let out = transactions(&graph, &scope(), "c-1", Some("issuance")).unwrap();
        assert_eq!(out["total"], json!(1));
        assert_eq!(out["transactions"][0]["id"], json!("tx-1"));
    }
}
