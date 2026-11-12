#![forbid(unsafe_code)]
//! fund-accounting-service, on the store.
//!
//! Money set aside for a purpose, and what has moved through it. A fund carries a balance and, derived
//! from its transactions, what it has taken in and spent, and therefore what it is worth.
//!
//! The arithmetic, all exact:
//!
//! * a fund's net assets are its balance plus income less expenses,
//! * recording a transaction recomputes both totals from **every** transaction the fund has, not by
//!   adding the new one, so the figures always describe the whole history,
//! * a company's funds total their balances and their net assets separately.
//!
//! Kept as shipped: a transaction never moves the fund's balance. Income and expenses are counted from
//! the transactions and the balance is whatever it was created with, so a fund that receives a
//! thousand shows a thousand more net assets and the same balance. That is the service's shape, and a
//! balance it silently corrected would disagree with funds already on record.

use serde_json::{json, Value};
use vimbai_common::Money;

use vimbai_store::amounts::{amount_or_zero, stored_amount, wire};
use vimbai_store::{BookScope, Graph, Records, StoreError, FUND, FUND_TRANSACTION};

fn text(payload: &Value, key: &str) -> String {
    payload
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// What a fund is worth: its balance, plus what came in, less what went out.
pub fn net_assets(balance: &Money, income: &Money, expenses: &Money) -> Money {
    balance.plus(income).minus(expenses)
}

/// Split a fund's transactions into what came in and what went out.
pub fn totals(transactions: &[Value]) -> Result<(Money, Money), StoreError> {
    let mut income = Money::zero();
    let mut expenses = Money::zero();
    for row in transactions {
        let amount = stored_amount(row, "amount")?;
        if row
            .get("is_income")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            income = income.plus(&amount);
        } else {
            expenses = expenses.plus(&amount);
        }
    }
    Ok((income, expenses))
}

/// `/funds`.
pub fn create_fund<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let balance = amount_or_zero(payload, "balance")?;
    let income = amount_or_zero(payload, "income")?;
    let expenses = amount_or_zero(payload, "expenses")?;
    let worth = net_assets(&balance, &income, &expenses);
    Records::new(graph, FUND).create(
        scope,
        &json!({
            "company_id": text(payload, "company_id"),
            "fund_name": text(payload, "fund_name"),
            "fund_type": if text(payload, "fund_type").is_empty() {
                "general".to_string()
            } else {
                text(payload, "fund_type")
            },
            "balance": wire(&balance),
            "income": wire(&income),
            "expenses": wire(&expenses),
            "net_assets": wire(&worth),
            "restrictions": text(payload, "restrictions"),
            "manager": text(payload, "manager"),
        }),
    )
}

/// `/funds/{company_id}`.
pub fn list_funds<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    company_id: &str,
) -> Result<Value, StoreError> {
    let rows = Records::new(graph, FUND).list(scope, 1000)?;
    let mine: Vec<Value> = rows
        .into_iter()
        .filter(|row| row.get("company_id").and_then(Value::as_str) == Some(company_id))
        .collect();
    let mut balance = Money::zero();
    let mut worth = Money::zero();
    for row in &mine {
        balance = balance.plus(&stored_amount(row, "balance")?);
        worth = worth.plus(&stored_amount(row, "net_assets")?);
    }
    Ok(json!({
        "company_id": company_id,
        "funds": mine,
        "total_balance": wire(&balance),
        "total_net_assets": wire(&worth),
    }))
}

/// `/transactions`: record the movement and re-derive the fund's aggregates from all of them.
pub fn record_transaction<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let fund_id = text(payload, "fund_id");
    let funds = Records::new(graph, FUND);
    let fund = funds.get(scope, &fund_id)?;
    let balance = stored_amount(&fund, "balance")?;

    let recorded = Records::new(graph, FUND_TRANSACTION).create(
        scope,
        &json!({
            "fund_id": fund_id,
            "description": text(payload, "description"),
            "amount": wire(&amount_or_zero(payload, "amount")?),
            "is_income": payload
                .get("is_income")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            "category": text(payload, "category"),
            "date": text(payload, "date"),
        }),
    )?;

    // the fund's figures describe its whole history, so they are recomputed rather than added to
    let history: Vec<Value> = Records::new(graph, FUND_TRANSACTION)
        .list(scope, 1000)?
        .into_iter()
        .filter(|row| row.get("fund_id").and_then(Value::as_str) == Some(fund_id.as_str()))
        .collect();
    let (income, expenses) = totals(&history)?;
    let worth = net_assets(&balance, &income, &expenses);

    funds.update(
        scope,
        &fund_id,
        &json!({
            "income": wire(&income),
            "expenses": wire(&expenses),
            "net_assets": wire(&worth),
        }),
    )?;

    Ok(json!({
        "id": crate::intercompany::id_of(&recorded),
        "status": "recorded",
    }))
}

/// `/transactions/{fund_id}`.
pub fn list_transactions<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    fund_id: &str,
) -> Result<Value, StoreError> {
    // reading a fund's transactions requires the fund itself to be visible
    Records::new(graph, FUND).get(scope, fund_id)?;
    let mut rows: Vec<Value> = Records::new(graph, FUND_TRANSACTION)
        .list(scope, 1000)?
        .into_iter()
        .filter(|row| row.get("fund_id").and_then(Value::as_str) == Some(fund_id))
        .collect();
    rows.sort_by_key(|row| {
        row.get("date")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    });
    Ok(json!({
        "fund_id": fund_id,
        "transactions": rows,
        "total": rows.len(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::FakeGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn money(text: &str) -> Money {
        Money::parse(text).expect("amount")
    }

    #[test]
    fn net_assets_are_the_balance_plus_income_less_expenses() {
        assert_eq!(
            wire(&net_assets(&money("1000"), &money("250"), &money("100"))),
            "1150.000000"
        );
    }

    #[test]
    fn totals_split_the_transactions_by_direction() {
        let rows = vec![
            json!({"amount": "250", "is_income": true}),
            json!({"amount": "100", "is_income": false}),
            json!({"amount": "50", "is_income": true}),
        ];
        let (income, expenses) = totals(&rows).expect("totals");
        assert_eq!(wire(&income), "300.000000");
        assert_eq!(wire(&expenses), "100.000000");
    }

    #[test]
    fn a_fund_is_created_with_its_net_assets_derived() {
        let graph = FakeGraph::returning(vec![json!({"id": "fund-1"})]);
        create_fund(
            &graph,
            &scope(),
            &json!({"company_id": "co-1", "fund_name": "Building fund",
                    "balance": "1000", "income": "250", "expenses": "100"}),
        )
        .expect("created");
        let (_, params) = graph.last();
        assert_eq!(params["props"]["net_assets"], json!("1150.000000"));
        assert_eq!(params["props"]["fund_type"], json!("general"));
    }

    #[test]
    fn a_transaction_leaves_the_balance_alone_and_rederives_the_rest() {
        let graph = FakeGraph::queue(vec![
            vec![json!({"id": "fund-1", "balance": "1000"})],
            vec![json!({"id": "tx-1"})],
            vec![
                json!({"id": "tx-1", "fund_id": "fund-1", "amount": "500", "is_income": true}),
                json!({"id": "tx-2", "fund_id": "fund-1", "amount": "200", "is_income": false}),
                json!({"id": "tx-3", "fund_id": "other", "amount": "999", "is_income": true}),
            ],
            vec![json!({"id": "fund-1"})],
        ]);
        record_transaction(
            &graph,
            &scope(),
            &json!({"fund_id": "fund-1", "description": "Grant", "amount": "500",
                    "is_income": true, "date": "2026-03-01"}),
        )
        .expect("recorded");
        let (_, params) = graph.statement(3);
        let patch = &params["patch"];
        assert_eq!(patch["income"], json!("500.000000"));
        assert_eq!(patch["expenses"], json!("200.000000"));
        assert_eq!(patch["net_assets"], json!("1300.000000"));
        assert!(!patch.as_object().expect("patch").contains_key("balance"));
    }

    #[test]
    fn a_movement_past_double_size_keeps_every_digit() {
        let graph = FakeGraph::queue(vec![
            vec![json!({"id": "fund-1", "balance": "123456789012345678901.55"})],
            vec![json!({"id": "tx-1"})],
            vec![json!({"id": "tx-1", "fund_id": "fund-1",
                        "amount": "0.45", "is_income": true})],
            vec![json!({"id": "fund-1"})],
        ]);
        record_transaction(
            &graph,
            &scope(),
            &json!({"fund_id": "fund-1", "amount": "0.45", "is_income": true}),
        )
        .expect("recorded");
        let (_, params) = graph.statement(3);
        assert_eq!(
            params["patch"]["net_assets"],
            json!("123456789012345678902.000000")
        );
    }

    #[test]
    fn a_transaction_against_an_unknown_fund_is_refused() {
        let graph = FakeGraph::returning(Vec::new());
        assert!(matches!(
            record_transaction(&graph, &scope(), &json!({"fund_id": "nope", "amount": "1"})),
            Err(StoreError::NotFound)
        ));
    }

    #[test]
    fn a_company_totals_its_funds_separately() {
        let graph = FakeGraph::returning(vec![
            json!({"company_id": "co-1", "balance": "1000", "net_assets": "1150"}),
            json!({"company_id": "co-1", "balance": "500", "net_assets": "400"}),
            json!({"company_id": "co-2", "balance": "9999", "net_assets": "9999"}),
        ]);
        let answer = list_funds(&graph, &scope(), "co-1").expect("funds");
        assert_eq!(answer["total_balance"], json!("1500.000000"));
        assert_eq!(answer["total_net_assets"], json!("1550.000000"));
        assert_eq!(answer["funds"].as_array().expect("funds").len(), 2);
    }
}
