#![forbid(unsafe_code)]
//! revenue-recognition-service, on the store.
//!
//! A contract with a customer is a set of performance obligations, each satisfied its own way. The
//! transaction price is split across them by relative standalone selling price, revenue is recognised
//! obligation by obligation as each is satisfied, and whatever is not yet recognised stays deferred.
//!
//! The arithmetic, all exact:
//!
//! * the contract's transaction price is the sum of what the caller priced each obligation at,
//! * allocation only runs when at least one obligation carries a standalone selling price: each
//!   obligation that has one is re-priced at its share of the total, and each that does not keeps the
//!   price it was given,
//! * recognised revenue on an obligation is capped at that obligation's price, so it cannot run past
//!   what was sold,
//! * deferred revenue is the contract price less everything recognised so far.
//!
//! Kept as shipped, and it is the reason allocation is documented rather than tightened: an obligation
//! with no standalone selling price keeps its own figure while the others are re-priced from the whole,
//! so the allocated parts need not add back to the total. That is what the service did, and a contract
//! already allocated that way would change underneath its own record if this were corrected.
//!
//! Also as shipped: a recognition call with no amount recognises the whole obligation remaining, not
//! zero. It means "recognise it in full", which is how the endpoint has always been used.

use serde_json::{json, Value};
use vimbai_common::Money;

use vimbai_store::amounts::{amount, amount_or_zero, stored_amount, wire};
use vimbai_store::{BookScope, Graph, Records, StoreError, REVENUE_CONTRACT};

fn text(payload: &Value, key: &str) -> String {
    payload
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn obligations(payload: &Value) -> Vec<Value> {
    payload
        .get("obligations")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

/// One obligation, with the figures the allocation and recognition steps work on.
#[derive(Clone, Debug)]
pub struct Obligation {
    pub description: String,
    pub price: Money,
    pub standalone: Money,
    pub method: String,
    pub satisfied: bool,
    pub recognised: Money,
}

impl Obligation {
    fn read(row: &Value) -> Result<Self, StoreError> {
        Ok(Self {
            description: text(row, "description"),
            price: amount_or_zero(row, "transaction_price")?,
            standalone: amount_or_zero(row, "standalone_selling_price")?,
            method: if text(row, "recognition_method").is_empty() {
                "point_in_time".to_string()
            } else {
                text(row, "recognition_method")
            },
            satisfied: row
                .get("is_satisfied")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            recognised: amount_or_zero(row, "revenue_recognized")?,
        })
    }

    fn to_json(&self) -> Value {
        json!({
            "description": self.description,
            "transaction_price": wire(&self.price),
            "standalone_selling_price": wire(&self.standalone),
            "recognition_method": self.method,
            "is_satisfied": self.satisfied,
            "revenue_recognized": wire(&self.recognised),
        })
    }
}

/// Split the transaction price by relative standalone selling price, where one exists.
///
/// An obligation with no standalone price keeps the price it was given. That is why the parts need not
/// add back to the total, and why this is a documented shape rather than a bug to fix.
pub fn allocate_price(total: &Money, rows: &mut [Obligation]) {
    let standalone_total = rows
        .iter()
        .fold(Money::zero(), |sum, row| sum.plus(&row.standalone));
    if standalone_total <= Money::zero() || *total <= Money::zero() {
        return;
    }
    for row in rows.iter_mut() {
        if row.standalone > Money::zero() {
            let share = row.standalone.over(&standalone_total);
            row.price = total.times(&share);
        }
    }
}

/// Recognise against one obligation, capped at what it is worth.
pub fn recognise(row: &mut Obligation, asked: &Money) -> Money {
    let amount = if *asked > Money::zero() {
        asked.clone()
    } else {
        row.price.clone()
    };
    let raised = row.recognised.plus(&amount);
    row.recognised = if raised > row.price {
        row.price.clone()
    } else {
        raised
    };
    row.satisfied = row.recognised >= row.price;
    row.recognised.clone()
}

fn read_rows(contract: &Value) -> Result<Vec<Obligation>, StoreError> {
    let rows = contract
        .get("obligations")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    rows.iter().map(Obligation::read).collect()
}

fn total_of(rows: &[Obligation]) -> Money {
    rows.iter()
        .fold(Money::zero(), |sum, row| sum.plus(&row.price))
}

fn recognised_of(rows: &[Obligation]) -> Money {
    rows.iter()
        .fold(Money::zero(), |sum, row| sum.plus(&row.recognised))
}

/// `/contracts`.
pub fn create_contract<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let mut rows: Vec<Obligation> = obligations(payload)
        .iter()
        .map(Obligation::read)
        .collect::<Result<_, _>>()?;

    // the contract price is what the caller priced the parts at, before any allocation
    let total = total_of(&rows);
    if rows.iter().any(|row| row.standalone > Money::zero()) {
        allocate_price(&total, &mut rows);
    }
    let recognised = recognised_of(&rows);
    let deferred = total.minus(&recognised);

    Records::new(graph, REVENUE_CONTRACT).create(
        scope,
        &json!({
            "company_id": text(payload, "company_id"),
            "customer_name": text(payload, "customer_name"),
            "contract_date": text(payload, "contract_date"),
            "total_transaction_price": wire(&total),
            "total_revenue_recognized": wire(&recognised),
            "deferred_revenue": wire(&deferred),
            "status": "active",
            "obligations": rows.iter().map(Obligation::to_json).collect::<Vec<_>>(),
        }),
    )
}

/// `/contracts/{id}/recognize`: recognise against one obligation and write the contract back.
pub fn recognise_revenue<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    contract_id: &str,
    obligation_id: &str,
    asked: &Money,
) -> Result<Value, StoreError> {
    let records = Records::new(graph, REVENUE_CONTRACT);
    let contract = records.get(scope, contract_id)?;
    let mut rows = read_rows(&contract)?;

    // obligations are addressed by their place in the list, as the store keeps no separate id on them
    let index = obligation_id
        .parse::<usize>()
        .map_err(|_| StoreError::NotFound)?;
    let row = rows.get_mut(index).ok_or(StoreError::NotFound)?;
    let recognised = recognise(row, asked);
    let satisfied = row.satisfied;

    let total = stored_amount(&contract, "total_transaction_price")?;
    let recognised_total = recognised_of(&rows);
    let deferred = total.minus(&recognised_total);

    records.update(
        scope,
        contract_id,
        &json!({
            "obligations": rows.iter().map(Obligation::to_json).collect::<Vec<_>>(),
            "total_revenue_recognized": wire(&recognised_total),
            "deferred_revenue": wire(&deferred),
        }),
    )?;

    Ok(json!({
        "obligation_id": obligation_id,
        "recognized": wire(&recognised),
        "is_satisfied": satisfied,
        "contract_total_recognized": wire(&recognised_total),
        "deferred": wire(&deferred),
    }))
}

/// `/contracts/{company_id}`.
pub fn list_contracts<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    company_id: &str,
) -> Result<Value, StoreError> {
    let rows = Records::new(graph, REVENUE_CONTRACT).list(scope, 1000)?;
    let mine: Vec<Value> = rows
        .into_iter()
        .filter(|row| row.get("company_id").and_then(Value::as_str) == Some(company_id))
        .collect();
    Ok(json!({
        "company_id": company_id,
        "contracts": mine,
        "total": mine.len(),
    }))
}

/// `/summary/{company_id}`.
pub fn revenue_summary<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    company_id: &str,
) -> Result<Value, StoreError> {
    let rows = Records::new(graph, REVENUE_CONTRACT).list(scope, 1000)?;
    let mine: Vec<Value> = rows
        .into_iter()
        .filter(|row| row.get("company_id").and_then(Value::as_str) == Some(company_id))
        .collect();
    let mut value = Money::zero();
    let mut recognised = Money::zero();
    let mut deferred = Money::zero();
    for row in &mine {
        value = value.plus(&stored_amount(row, "total_transaction_price")?);
        recognised = recognised.plus(&stored_amount(row, "total_revenue_recognized")?);
        deferred = deferred.plus(&stored_amount(row, "deferred_revenue")?);
    }
    Ok(json!({
        "company_id": company_id,
        "total_contracts": mine.len(),
        "total_contract_value": wire(&value),
        "revenue_recognized": wire(&recognised),
        "deferred_revenue": wire(&deferred),
    }))
}

/// Read an amount off a payload, refusing a missing one where the endpoint requires it.
pub fn required_amount(payload: &Value, key: &str) -> Result<Money, StoreError> {
    amount(payload, key)
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

    fn row(price: &str, standalone: &str) -> Obligation {
        Obligation {
            description: "obligation".into(),
            price: money(price),
            standalone: money(standalone),
            method: "point_in_time".into(),
            satisfied: false,
            recognised: Money::zero(),
        }
    }

    #[test]
    fn allocation_splits_the_price_by_relative_standalone_value() {
        let mut rows = vec![row("100", "80"), row("100", "20")];
        allocate_price(&money("200"), &mut rows);
        assert_eq!(wire(&rows[0].price), "160.000000");
        assert_eq!(wire(&rows[1].price), "40.000000");
    }

    #[test]
    fn an_obligation_without_a_standalone_price_keeps_the_price_it_was_given() {
        let mut rows = vec![row("100", "80"), row("100", "0")];
        allocate_price(&money("200"), &mut rows);
        assert_eq!(wire(&rows[0].price), "200.000000");
        assert_eq!(wire(&rows[1].price), "100.000000");
        // the parts no longer add back to the total: shipped behaviour, pinned here
        let parts = total_of(&rows);
        assert_eq!(wire(&parts), "300.000000");
    }

    #[test]
    fn recognition_stops_at_what_the_obligation_is_worth() {
        let mut only = row("100", "0");
        recognise(&mut only, &money("150"));
        assert_eq!(wire(&only.recognised), "100.000000");
        assert!(only.satisfied);
    }

    #[test]
    fn a_call_with_no_amount_recognises_the_obligation_in_full() {
        let mut only = row("40", "0");
        recognise(&mut only, &Money::zero());
        assert_eq!(wire(&only.recognised), "40.000000");
        assert!(only.satisfied);
    }

    #[test]
    fn creating_a_contract_records_its_price_and_what_is_deferred() {
        let graph = FakeGraph::returning(vec![json!({"id": "con-1"})]);
        create_contract(
            &graph,
            &scope(),
            &json!({
                "company_id": "co-1", "customer_name": "Acme", "contract_date": "2026-03-01",
                "obligations": [
                    {"description": "Install", "transaction_price": "1000", "standalone_selling_price": "1200"},
                    {"description": "Support", "transaction_price": "1000", "standalone_selling_price": "800"}
                ]
            }),
        )
        .expect("created");
        let (_, params) = graph.last();
        let written = &params["props"];
        assert_eq!(written["total_transaction_price"], json!("2000.000000"));
        assert_eq!(written["deferred_revenue"], json!("2000.000000"));
        let rows = written["obligations"].as_array().expect("rows");
        assert_eq!(rows[0]["transaction_price"], json!("1200.000000"));
        assert_eq!(rows[1]["transaction_price"], json!("800.000000"));
    }

    #[test]
    fn recognising_writes_the_contract_back_with_what_is_left_deferred() {
        let graph = FakeGraph::queue(vec![
            vec![json!({
                "id": "con-1", "total_transaction_price": "2000",
                "obligations": [
                    {"description": "Install", "transaction_price": "1200",
                     "standalone_selling_price": "1200"},
                    {"description": "Support", "transaction_price": "800",
                     "standalone_selling_price": "800"}
                ]
            })],
            vec![json!({"id": "con-1"})],
        ]);
        let answer =
            recognise_revenue(&graph, &scope(), "con-1", "0", &money("500")).expect("recognised");
        assert_eq!(answer["recognized"], json!("500.000000"));
        assert_eq!(answer["contract_total_recognized"], json!("500.000000"));
        assert_eq!(answer["deferred"], json!("1500.000000"));
        assert_eq!(answer["is_satisfied"], json!(false));
        let (_, params) = graph.statement(1);
        assert_eq!(params["patch"]["deferred_revenue"], json!("1500.000000"));
    }

    #[test]
    fn a_contract_past_double_size_keeps_every_digit() {
        let graph = FakeGraph::queue(vec![
            vec![json!({
                "id": "con-1", "total_transaction_price": "123456789012345678901.55",
                "obligations": [{"description": "One", "transaction_price":
                                 "123456789012345678901.55", "standalone_selling_price": "0"}]
            })],
            vec![json!({"id": "con-1"})],
        ]);
        let answer =
            recognise_revenue(&graph, &scope(), "con-1", "0", &money("0.45")).expect("recognised");
        assert_eq!(answer["recognized"], json!("0.450000"));
        assert_eq!(answer["deferred"], json!("123456789012345678901.100000"));
    }

    #[test]
    fn an_obligation_that_does_not_exist_is_refused() {
        let graph = FakeGraph::returning(vec![json!({
            "id": "con-1", "total_transaction_price": "10", "obligations": []
        })]);
        assert!(matches!(
            recognise_revenue(&graph, &scope(), "con-1", "3", &money("1")),
            Err(StoreError::NotFound)
        ));
    }
}
