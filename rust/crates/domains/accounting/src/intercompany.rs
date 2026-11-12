#![forbid(unsafe_code)]
//! intercompany-service, on the store.
//!
//! Entities inside one group, the transactions between them, and the eliminations that take those
//! transactions back out of the consolidated figures.
//!
//! The arithmetic, all exact:
//!
//! * a transaction must be one of loan, service fee, royalty, sale or cost allocation, and is recorded
//!   against the two entities at an exact amount,
//! * two transactions match when their amounts are equal, and matching marks both as matched and writes
//!   one elimination for the pair,
//! * the elimination's amount is the matched amount.
//!
//! Kept as shipped, and pinned by tests, because these are what the stored eliminations already say:
//!
//! * a match is exact equality, with no tolerance and no currency check, so 100 USD and 100 EUR match;
//! * matching never checks that the two transactions face each other, so two same-direction legs match
//!   as readily as a mirrored pair;
//! * the elimination takes its debit from the first transaction's from-entity and its credit from the
//!   second transaction's from-entity. For a true mirror that is the same entity pair the other way
//!   round; for anything else it credits the wrong side.

use serde_json::{json, Value};

use vimbai_store::amounts::{amount_or_zero, stored_amount, wire};
use vimbai_store::{
    BookScope, Graph, Records, StoreError, INTERCOMPANY_ELIMINATION, INTERCOMPANY_ENTITY,
    INTERCOMPANY_TRANSACTION,
};

/// The types a transaction may be.
pub const TYPES: [&str; 5] = ["loan", "service_fee", "royalty", "sale", "cost_allocation"];

fn text(payload: &Value, key: &str) -> String {
    payload
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn or_default(payload: &Value, key: &str, fallback: &str) -> String {
    let given = text(payload, key);
    if given.is_empty() {
        fallback.to_string()
    } else {
        given
    }
}

fn optional(payload: &Value, key: &str) -> Option<String> {
    let given = text(payload, key);
    if given.is_empty() {
        None
    } else {
        Some(given)
    }
}

/// `/entities`.
pub fn create_entity<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    Records::new(graph, INTERCOMPANY_ENTITY).create(
        scope,
        &json!({
            "name": text(payload, "name"),
            "legal_entity_code": text(payload, "legal_entity_code"),
            "tax_jurisdiction": text(payload, "tax_jurisdiction"),
            "currency": or_default(payload, "currency", "USD"),
            "status": "active",
        }),
    )
}

/// `/entities` listing.
pub fn list_entities<G: Graph + ?Sized>(graph: &G, scope: &BookScope) -> Result<Value, StoreError> {
    let rows = Records::new(graph, INTERCOMPANY_ENTITY).list(scope, 1000)?;
    Ok(Value::Array(rows))
}

/// `/transactions`.
pub fn create_transaction<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let kind = text(payload, "transaction_type");
    if !TYPES.contains(&kind.as_str()) {
        return Err(StoreError::Invalid(format!(
            "Invalid type. Must be one of {:?}",
            TYPES
        )));
    }
    Records::new(graph, INTERCOMPANY_TRANSACTION).create(
        scope,
        &json!({
            "from_entity_id": text(payload, "from_entity_id"),
            "to_entity_id": text(payload, "to_entity_id"),
            "transaction_type": kind,
            "amount": wire(&amount_or_zero(payload, "amount")?),
            "currency": or_default(payload, "currency", "USD"),
            "description": text(payload, "description"),
            "transfer_price_basis": or_default(payload, "transfer_price_basis", "cost_plus"),
            "status": "pending",
        }),
    )
}

/// `/transactions` listing, filtered as the service filtered.
pub fn list_transactions<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let from = optional(query, "from_entity");
    let to = optional(query, "to_entity");
    let status = optional(query, "status");
    let rows = Records::new(graph, INTERCOMPANY_TRANSACTION).list(scope, 1000)?;
    let kept: Vec<Value> = rows
        .into_iter()
        .filter(|row| {
            from.as_deref()
                .map(|want| row.get("from_entity_id").and_then(Value::as_str) == Some(want))
                .unwrap_or(true)
                && to
                    .as_deref()
                    .map(|want| row.get("to_entity_id").and_then(Value::as_str) == Some(want))
                    .unwrap_or(true)
                && status
                    .as_deref()
                    .map(|want| row.get("status").and_then(Value::as_str) == Some(want))
                    .unwrap_or(true)
        })
        .collect();
    Ok(Value::Array(kept))
}

/// `/transactions/match`: mark both legs, then eliminate the pair.
pub fn match_transactions<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let first_id = text(payload, "txn1_id");
    let second_id = text(payload, "txn2_id");
    let records = Records::new(graph, INTERCOMPANY_TRANSACTION);
    let first = records.get(scope, &first_id)?;
    let second = records.get(scope, &second_id)?;

    // equality of the stored amounts, exactly: no tolerance, and no currency check
    let first_amount = stored_amount(&first, "amount")?;
    let second_amount = stored_amount(&second, "amount")?;
    if first_amount != second_amount {
        return Err(StoreError::Invalid("Amounts do not match".into()));
    }

    let matched = json!({
        "status": "matched",
    });
    let mut first_patch = matched.clone();
    first_patch["matched_transaction_id"] = json!(second_id);
    records.update(scope, &first_id, &first_patch)?;
    let mut second_patch = matched;
    second_patch["matched_transaction_id"] = json!(first_id);
    records.update(scope, &second_id, &second_patch)?;

    let elimination = Records::new(graph, INTERCOMPANY_ELIMINATION).create(
        scope,
        &json!({
            "pair_id": format!("{first_id}:{second_id}"),
            // both sides read from the from-entity of each leg, as shipped
            "debit_entity_id": text(&first, "from_entity_id"),
            "credit_entity_id": text(&second, "from_entity_id"),
            "amount": wire(&first_amount),
            "description": format!("Elimination: {}", text(&first, "description")),
        }),
    )?;

    Ok(json!({
        "matched": true,
        "elimination_id": id_of(&elimination),
    }))
}

/// The id of a created record, which the driver returns either as the record or as its id.
pub(crate) fn id_of(created: &Value) -> Value {
    match created {
        Value::Object(fields) => fields.get("id").cloned().unwrap_or(Value::Null),
        Value::String(id) => json!(id),
        _ => Value::Null,
    }
}

/// `/eliminations`.
pub fn list_eliminations<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
) -> Result<Value, StoreError> {
    let rows = Records::new(graph, INTERCOMPANY_ELIMINATION).list(scope, 1000)?;
    Ok(Value::Array(rows))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::FakeGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn leg(id: &str, from: &str, to: &str, amount: &str) -> Value {
        json!({"id": id, "from_entity_id": from, "to_entity_id": to,
               "amount": amount, "description": "Management fee", "status": "pending"})
    }

    #[test]
    fn only_the_five_known_types_are_accepted() {
        for kind in TYPES {
            let graph = FakeGraph::returning(vec![json!({"id": "tx-1"})]);
            create_transaction(
                &graph,
                &scope(),
                &json!({"from_entity_id": "a", "to_entity_id": "b",
                        "transaction_type": kind, "amount": "100"}),
            )
            .expect("accepted");
        }
        let graph = FakeGraph::returning(vec![json!({"id": "tx-1"})]);
        assert!(matches!(
            create_transaction(
                &graph,
                &scope(),
                &json!({"from_entity_id": "a", "to_entity_id": "b",
                        "transaction_type": "gift", "amount": "100"}),
            ),
            Err(StoreError::Invalid(_))
        ));
    }

    #[test]
    fn transactions_are_recorded_at_an_exact_amount() {
        let graph = FakeGraph::returning(vec![json!({"id": "tx-1"})]);
        create_transaction(
            &graph,
            &scope(),
            &json!({"from_entity_id": "a", "to_entity_id": "b", "transaction_type": "loan",
                    "amount": "123456789012345678901.55"}),
        )
        .expect("created");
        let (_, params) = graph.last();
        assert_eq!(
            params["props"]["amount"],
            json!("123456789012345678901.550000")
        );
        assert_eq!(params["props"]["status"], json!("pending"));
        assert_eq!(params["props"]["transfer_price_basis"], json!("cost_plus"));
    }

    #[test]
    fn matching_marks_both_legs_and_writes_one_elimination() {
        let graph = FakeGraph::queue(vec![
            vec![leg("tx-1", "a", "b", "500")],
            vec![leg("tx-2", "b", "a", "500")],
            vec![json!({"id": "tx-1"})],
            vec![json!({"id": "tx-2"})],
            vec![json!({"id": "el-1"})],
        ]);
        let answer = match_transactions(
            &graph,
            &scope(),
            &json!({"txn1_id": "tx-1", "txn2_id": "tx-2"}),
        )
        .expect("matched");
        assert_eq!(answer["matched"], json!(true));
        assert_eq!(answer["elimination_id"], json!("el-1"));
        let (_, elimination) = graph.statement(4);
        assert_eq!(elimination["props"]["pair_id"], json!("tx-1:tx-2"));
        assert_eq!(elimination["props"]["amount"], json!("500.000000"));
        assert_eq!(elimination["props"]["debit_entity_id"], json!("a"));
        // the credit side is the second leg's from-entity, which is the first leg's counterparty
        assert_eq!(elimination["props"]["credit_entity_id"], json!("b"));
    }

    #[test]
    fn amounts_that_differ_by_a_cent_do_not_match() {
        let graph = FakeGraph::queue(vec![
            vec![leg("tx-1", "a", "b", "500.00")],
            vec![leg("tx-2", "b", "a", "500.01")],
        ]);
        assert!(matches!(
            match_transactions(
                &graph,
                &scope(),
                &json!({"txn1_id": "tx-1", "txn2_id": "tx-2"})
            ),
            Err(StoreError::Invalid(_))
        ));
    }

    #[test]
    fn two_legs_in_the_same_direction_still_match_and_eliminate() {
        // as shipped: nothing checks that the legs face each other
        let graph = FakeGraph::queue(vec![
            vec![leg("tx-1", "a", "b", "500")],
            vec![leg("tx-2", "a", "b", "500")],
            vec![json!({"id": "tx-1"})],
            vec![json!({"id": "tx-2"})],
            vec![json!({"id": "el-1"})],
        ]);
        match_transactions(
            &graph,
            &scope(),
            &json!({"txn1_id": "tx-1", "txn2_id": "tx-2"}),
        )
        .expect("matched");
        let (_, elimination) = graph.statement(4);
        // both sides are entity a, which is what the service wrote
        assert_eq!(elimination["props"]["debit_entity_id"], json!("a"));
        assert_eq!(elimination["props"]["credit_entity_id"], json!("a"));
    }

    #[test]
    fn a_transaction_that_is_not_there_is_not_found() {
        let graph = FakeGraph::queue(vec![vec![leg("tx-1", "a", "b", "500")], vec![]]);
        assert!(matches!(
            match_transactions(
                &graph,
                &scope(),
                &json!({"txn1_id": "tx-1", "txn2_id": "gone"})
            ),
            Err(StoreError::NotFound)
        ));
    }

    #[test]
    fn the_transaction_listing_filters_as_asked() {
        let graph = FakeGraph::returning(vec![
            json!({"from_entity_id": "a", "to_entity_id": "b", "status": "matched"}),
            json!({"from_entity_id": "a", "to_entity_id": "c", "status": "pending"}),
            json!({"from_entity_id": "b", "to_entity_id": "a", "status": "pending"}),
        ]);
        let answer = list_transactions(
            &graph,
            &scope(),
            &json!({"from_entity": "a", "status": "pending"}),
        )
        .expect("listed");
        assert_eq!(answer.as_array().expect("rows").len(), 1);
        assert_eq!(answer[0]["to_entity_id"], json!("c"));
    }
}
