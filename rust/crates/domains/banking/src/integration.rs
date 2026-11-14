#![forbid(unsafe_code)]
//! banking-integration-service: the plain connection to a bank account, the sync over it and the reconciliation
//! of what arrived.
//!
//! This service and bank-feed-service are two front doors to one idea - a bank account the Book has a connection
//! to, and the transactions that arrive over it - so they share one record kind each rather than keeping two
//! sets of rows that cannot see one another:
//!
//! * a connection lives under `BankConnection` with one edge, whichever door opened it. This service states
//!   `bank_name` and `account_number`; bank-feed states `provider` and the last four digits. A field the other
//!   door recorded is simply empty, and each door answers what it needs with defaults;
//! * the transactions are the feed's transactions. A transaction reconciled here is reconciled for the feed
//!   too, because it is one row.
//!
//! Behaviours kept as shipped, each pinned by a test:
//!
//! * a connection to the same bank and account number is refused as a duplicate rather than opened twice;
//! * a sync is refused when the connection is not active, so a disconnected account cannot be swept;
//! * a sync stamps the connection with the moment it ran and answers it;
//! * a transaction can only be reconciled through the connection it belongs to - naming another connection's
//!   transaction is a miss, not a reconciliation;
//! * disconnecting marks the connection disconnected and keeps the record, as shipped.

use serde_json::{json, Value};

use vimbai_store::amounts::wire;
use vimbai_store::read::{field, label, refused, text, whole};
use vimbai_store::{BookScope, Graph, Records, StoreError, BANK_CONNECTION, BANK_FEED_TRANSACTION};

/// The most records a listing reads in one go.
const LARGEST_PAGE: usize = 2000;

/// The account types the shipped service accepted by default.
const ACCOUNT_TYPES: [&str; 4] = ["checking", "savings", "credit", "loan"];

fn connections(graph: &(impl Graph + ?Sized), scope: &BookScope) -> Result<Vec<Value>, StoreError> {
    Records::new(graph, BANK_CONNECTION).list(scope, LARGEST_PAGE)
}

fn owned_connection(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    connection_id: &str,
) -> Result<Value, StoreError> {
    Records::new(graph, BANK_CONNECTION)
        .get(scope, connection_id)
        .map_err(|_| StoreError::NotFound)
}

/// `POST /connect`: one connection per bank and account number.
pub fn connect(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let bank_name = text(payload, "bank_name");
    let account_number = text(payload, "account_number");
    if bank_name.is_empty() || account_number.is_empty() {
        return Err(refused("bank_name and account_number are required"));
    }
    let account_type = match text(payload, "account_type") {
        stated if stated.is_empty() => "checking".to_string(),
        stated => {
            let lowered = stated.to_lowercase();
            if !ACCOUNT_TYPES.contains(&lowered.as_str()) {
                return Err(refused(&format!("Unknown account_type {stated}")));
            }
            lowered
        }
    };
    let already = connections(graph, scope)?.into_iter().any(|held| {
        field(&held, "bank_name") == bank_name && field(&held, "account_number") == account_number
    });
    if already {
        return Err(StoreError::Conflict(format!(
            "Connection to {bank_name} account {account_number} already exists"
        )));
    }
    let today = label(payload, "today");
    Records::new(graph, BANK_CONNECTION).create(
        scope,
        &json!({
            "bank_name": bank_name,
            "account_number": account_number,
            "account_type": account_type,
            "api_key": text(payload, "api_key"),
            "status": "active",
            "last_sync": Value::Null,
            "created_at": today,
            // the fields bank-feed's own door states, left empty here rather than invented
            "provider": "manual",
            "organization_id": text(payload, "organization_id"),
        }),
    )
}

/// `GET /connections`: every connection the Book holds, whichever door opened it.
pub fn list_connections(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
) -> Result<Value, StoreError> {
    let mine = connections(graph, scope)?;
    Ok(Value::Array(mine))
}

/// `POST /sync/{connection_id}`: the moment the account was swept, recorded on the connection.
pub fn sync(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    connection_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let held = owned_connection(graph, scope, connection_id)?;
    if field(&held, "status") != "active" {
        return Err(refused("Connection is not active"));
    }
    let today = label(payload, "today");
    Records::new(graph, BANK_CONNECTION).update(
        scope,
        connection_id,
        &json!({
            "last_sync": today,
            "last_sync_status": "completed",
        }),
    )?;
    Ok(json!({
        "connection_id": connection_id,
        "synced_at": today,
        "status": "success",
    }))
}

/// `GET /transactions/{connection_id}`: what arrived over one connection, paged.
pub fn list_connection_transactions(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    connection_id: &str,
    query: &Value,
) -> Result<Value, StoreError> {
    owned_connection(graph, scope, connection_id)?;
    let limit = match whole(query, "limit") {
        0 => 50,
        stated => stated,
    } as usize;
    let offset = whole(query, "offset") as usize;
    let mut mine: Vec<Value> = Records::new(graph, BANK_FEED_TRANSACTION)
        .list(scope, LARGEST_PAGE)?
        .into_iter()
        .filter(|held| field(held, "bank_connection_id") == connection_id)
        .collect();
    mine.sort_by(|left, right| field(right, "date").cmp(field(left, "date")));
    Ok(Value::Array(
        mine.into_iter().skip(offset).take(limit).collect(),
    ))
}

/// `POST /transactions/{connection_id}/reconcile`: a transaction matched to the entry it turned out to be.
pub fn reconcile_bank_transaction(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    connection_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let transaction_id = text(payload, "transaction_id");
    if transaction_id.is_empty() {
        return Err(refused("transaction_id is required"));
    }
    let held = Records::new(graph, BANK_FEED_TRANSACTION)
        .get(scope, &transaction_id)
        .map_err(|_| StoreError::NotFound)?;
    // the transaction has to belong to the connection named in the path, not merely to this Book
    if field(&held, "bank_connection_id") != connection_id {
        return Err(StoreError::NotFound);
    }
    let matched = text(payload, "matched_entry_id");
    Records::new(graph, BANK_FEED_TRANSACTION).update(
        scope,
        &transaction_id,
        &json!({
            "reconciled": true,
            "status": "reconciled",
            "matched_entry_id": matched,
            "reconciled_at": label(payload, "today"),
        }),
    )?;
    Ok(json!({
        "transaction_id": transaction_id,
        "reconciled": true,
        "matched_entry": matched,
    }))
}

/// `DELETE /connections/{connection_id}`: the connection stops being swept, and the record stays.
pub fn disconnect(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    connection_id: &str,
    today: &str,
) -> Result<Value, StoreError> {
    owned_connection(graph, scope, connection_id)?;
    Records::new(graph, BANK_CONNECTION).update(
        scope,
        connection_id,
        &json!({ "status": "disconnected", "disconnected_at": today }),
    )?;
    Ok(json!({
        "connection_id": connection_id,
        "status": "disconnected",
    }))
}

/// The figure a connection reports, for a caller that asks the integration door rather than the feed's.
pub fn balance_of(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    connection_id: &str,
) -> Result<Value, StoreError> {
    let held = owned_connection(graph, scope, connection_id)?;
    Ok(json!({
        "connection_id": connection_id,
        "bank_name": field(&held, "bank_name"),
        "status": field(&held, "status"),
        "last_sync": held.get("last_sync").cloned().unwrap_or(Value::Null),
        // this door never fetched a balance: the provider calls were a stub in the shipped service
        "source": "placeholder",
    }))
}

/// An amount as the engine writes one, exposed so the route layer does not re-format figures itself.
pub fn amount_text(payload: &Value, key: &str) -> Value {
    match vimbai_store::amounts::amount(payload, key) {
        Ok(held) => wire(&held),
        Err(_) => Value::Null,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::FakeGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    #[test]
    fn a_disconnected_account_is_not_swept() {
        let graph = vimbai_store::fake::MemoryGraph::new();
        let opened = connect(
            &graph,
            &scope(),
            &json!({"bank_name": "CBZ", "account_number": "0123456789", "today": "2026-11-14"}),
        )
        .expect("the connection opens");
        let id = opened["id"].as_str().unwrap_or_default().to_string();

        disconnect(&graph, &scope(), &id, "2026-11-14").expect("the connection is closed");

        let refused_sweep = sync(&graph, &scope(), &id, &json!({"today": "2026-11-14"}));
        match refused_sweep {
            Ok(_) => panic!("a closed account was swept"),
            Err(error) => assert!(
                error.to_string().contains("Connection is not active"),
                "{error}"
            ),
        }
    }

    #[test]
    fn a_second_connection_to_the_same_account_is_refused() {
        let graph = FakeGraph::queue(vec![vec![json!({
            "id": "conn-1", "bank_name": "CBZ", "account_number": "1234"
        })]]);
        let refused_twice = connect(
            &graph,
            &scope(),
            &json!({"bank_name": "CBZ", "account_number": "1234"}),
        );
        match refused_twice {
            Ok(_) => panic!("the duplicate was opened twice"),
            Err(error) => assert!(
                error
                    .to_string()
                    .contains("Connection to CBZ account 1234 already exists"),
                "{error}"
            ),
        }
    }

    #[test]
    fn a_connection_opens_against_an_account_it_can_be_asked_about() {
        let graph = FakeGraph::queue(vec![
            // nothing holds this account yet
            vec![],
            // the record as it was written
            vec![json!({
                "id": "conn-1", "bank_name": "CBZ", "account_number": "1234",
                "account_type": "savings", "status": "active", "provider": "manual"
            })],
        ]);
        let opened = connect(
            &graph,
            &scope(),
            &json!({"bank_name": "CBZ", "account_number": "1234", "account_type": "savings"}),
        )
        .expect("opened");
        assert_eq!(opened["status"], json!("active"));
        assert_eq!(opened["account_type"], json!("savings"));
    }

    #[test]
    fn a_sync_of_a_disconnected_account_is_refused() {
        let graph = FakeGraph::queue(vec![vec![json!({
            "id": "conn-1", "status": "disconnected"
        })]]);
        let refused_sync = sync(&graph, &scope(), "conn-1", &json!({}));
        assert!(refused_sync.is_err());
    }

    #[test]
    fn a_sync_records_its_moment_on_the_connection() {
        let graph = FakeGraph::queue(vec![
            vec![json!({"id": "conn-1", "status": "active"})],
            vec![json!({"id": "conn-1", "last_sync": "2026-03-02T09:00:00"})],
        ]);
        let answered = sync(
            &graph,
            &scope(),
            "conn-1",
            &json!({"today": "2026-03-02T09:00:00"}),
        )
        .expect("synced");
        assert_eq!(answered["synced_at"], json!("2026-03-02T09:00:00"));
        assert_eq!(answered["status"], json!("success"));
    }

    #[test]
    fn a_transaction_is_only_reconciled_through_its_own_connection() {
        let graph = FakeGraph::queue(vec![
            // the transaction, which belongs to another connection
            vec![json!({"id": "tx-1", "bank_connection_id": "conn-2"})],
        ]);
        let refused_reconcile = reconcile_bank_transaction(
            &graph,
            &scope(),
            "conn-1",
            &json!({"transaction_id": "tx-1"}),
        );
        assert!(refused_reconcile.is_err());
    }

    #[test]
    fn reconciling_a_transaction_marks_it_reconciled_for_both_doors() {
        let graph = FakeGraph::queue(vec![
            // the transaction, which names the connection it arrived on
            vec![json!({"id": "tx-1", "bank_connection_id": "conn-1", "status": "cleared"})],
            // the transaction, written back as reconciled
            vec![json!({"id": "tx-1", "bank_connection_id": "conn-1", "status": "reconciled"})],
        ]);
        let answered = reconcile_bank_transaction(
            &graph,
            &scope(),
            "conn-1",
            &json!({"transaction_id": "tx-1", "matched_entry_id": "je-1"}),
        )
        .expect("reconciled");
        assert_eq!(answered["reconciled"], json!(true));
        assert_eq!(answered["matched_entry"], json!("je-1"));
    }

    #[test]
    fn disconnecting_keeps_the_connection() {
        let graph = FakeGraph::queue(vec![
            vec![json!({"id": "conn-1", "status": "active"})],
            vec![json!({"id": "conn-1", "status": "disconnected"})],
        ]);
        let answered =
            disconnect(&graph, &scope(), "conn-1", "2026-03-02T09:00:00").expect("closed");
        assert_eq!(answered["status"], json!("disconnected"));
    }

    #[test]
    fn the_transactions_of_one_connection_are_paged() {
        let graph = FakeGraph::queue(vec![
            vec![json!({"id": "conn-1", "status": "active"})],
            vec![
                json!({"id": "tx-1", "bank_connection_id": "conn-1", "date": "2026-03-03"}),
                json!({"id": "tx-2", "bank_connection_id": "conn-1", "date": "2026-03-02"}),
                json!({"id": "tx-3", "bank_connection_id": "conn-1", "date": "2026-03-01"}),
                json!({"id": "tx-4", "bank_connection_id": "conn-9", "date": "2026-03-04"}),
            ],
        ]);
        let page = list_connection_transactions(&graph, &scope(), "conn-1", &json!({"limit": 2}))
            .expect("a page");
        assert_eq!(page.as_array().expect("a list").len(), 2);
        // newest first, and the other connection's transaction is not here
        assert_eq!(page[0]["id"], json!("tx-1"));
    }
}
