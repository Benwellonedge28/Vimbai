//! the bank feed over HTTP: connections, the transactions they bring in, and the rules that pick them out.

use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use serde_json::{json, Value};
use store::{Graph, StoreError};
use tower::ServiceExt;

struct Scripted {
    queue: Mutex<Vec<Vec<Value>>>,
}

impl Scripted {
    fn queue(scripts: Vec<Vec<Value>>) -> Arc<Self> {
        Arc::new(Self {
            queue: Mutex::new(scripts),
        })
    }
}

impl Graph for Scripted {
    fn run(&self, _cypher: &str, _params: Value) -> Result<Vec<Value>, StoreError> {
        let mut queue = self.queue.lock().unwrap();
        if queue.is_empty() {
            return Ok(Vec::new());
        }
        Ok(queue.remove(0))
    }
}

async fn call(
    graph: Arc<Scripted>,
    method: Method,
    uri: &str,
    payload: Option<Value>,
) -> (StatusCode, Value) {
    let router = vimbai_api::records::router(graph);
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .header("x-user-id", "user-1")
        .header("x-book-id", "book-1");
    let body = match payload {
        Some(value) => Body::from(value.to_string()),
        None => Body::empty(),
    };
    let response = router.oneshot(request.body(body).unwrap()).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1_000_000)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn send(graph: Arc<Scripted>, uri: &str, body: Value) -> (StatusCode, Value) {
    call(graph, Method::POST, uri, Some(body)).await
}

#[tokio::test]
async fn a_connection_is_opened_in_an_organisation() {
    let graph = Scripted::queue(vec![vec![json!({
        "id": "conn-1", "provider": "plaid", "organization_id": "org-1",
        "status": "active", "sync_interval_minutes": 60
    })]]);
    let (status, body) = send(
        graph,
        "/connections?organization_id=org-1",
        json!({
            "provider": "plaid",
            "account_name": "Current account",
            "account_type": "checking",
            "account_number_last4": "1234"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["organization_id"], json!("org-1"));
    assert_eq!(body["status"], json!("active"));
    assert_eq!(body["sync_interval_minutes"], json!(60));
}

#[tokio::test]
async fn a_provider_the_service_does_not_know_is_refused() {
    let graph = Scripted::queue(vec![]);
    let (status, body) = send(
        graph,
        "/connections?organization_id=org-1",
        json!({
            "provider": "telepathy",
            "account_name": "Mind reading",
            "account_number_last4": "0000"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["error"], json!("Unknown provider telepathy"));
}

#[tokio::test]
async fn a_transaction_is_imported_against_a_connection_the_book_holds() {
    let graph = Scripted::queue(vec![
        // the connection the transaction names
        vec![json!({"id": "conn-1", "provider": "plaid"})],
        // no transaction carries this external id yet
        vec![],
        // the imported transaction
        vec![json!({"id": "tx-1", "external_id": "ext-1", "status": "cleared"})],
    ]);
    let (status, body) = send(
        graph,
        "/bank-transactions/import",
        json!({
            "bank_connection_id": "conn-1",
            "external_id": "ext-1",
            "date": "2026-03-01T00:00:00",
            "amount": 500,
            "description": "card payment"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["status"], json!("cleared"));
}

#[tokio::test]
async fn the_same_external_id_twice_is_a_duplicate() {
    let graph = Scripted::queue(vec![
        vec![json!({"id": "conn-1", "provider": "plaid"})],
        vec![json!({"id": "tx-1", "external_id": "ext-1", "bank_connection_id": "conn-1"})],
    ]);
    let (status, body) = send(
        graph,
        "/bank-transactions/import",
        json!({
            "bank_connection_id": "conn-1",
            "external_id": "ext-1",
            "date": "2026-03-01T00:00:00",
            "amount": 500
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"], json!("Transaction already imported"));
}

#[tokio::test]
async fn an_mt940_statement_is_parsed_and_its_lines_imported() {
    let statement = ":20:ST\n:61:2603010301C1500,00NTRF\n:86:Customer payment\n";
    let graph = Scripted::queue(vec![
        // the connection named
        vec![json!({"id": "conn-1", "provider": "manual"})],
        // the imported line
        vec![json!({"id": "tx-1"})],
    ]);
    let (status, body) = send(
        graph,
        "/bank-transactions/import-mt940",
        json!({"connection_id": "conn-1", "statement_data": statement}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["parsed"], json!(1));
    assert_eq!(body["imported"], json!(1));
}

#[tokio::test]
async fn the_transactions_a_feed_brought_in_are_listed_for_one_connection() {
    let graph = Scripted::queue(vec![vec![
        json!({"id": "tx-1", "bank_connection_id": "conn-1", "status": "cleared", "date": "2026-03-02"}),
        json!({"id": "tx-2", "bank_connection_id": "conn-1", "status": "reconciled", "date": "2026-03-01"}),
        json!({"id": "tx-3", "bank_connection_id": "conn-2", "status": "cleared", "date": "2026-03-03"}),
    ]]);
    let (status, body) = call(
        graph,
        Method::GET,
        "/bank-transactions?connection_id=conn-1",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body.as_array().expect("a list").len(), 2);
    // newest first
    assert_eq!(body[0]["id"], json!("tx-1"));
}

#[tokio::test]
async fn the_rules_are_applied_to_the_transactions_named() {
    let graph = Scripted::queue(vec![
        // the caller's rules
        vec![json!({
            "id": "rule-1", "priority": 5, "active": true, "auto_match_enabled": true,
            "match_conditions": {"transaction_type": "debit"}
        })],
        // the transaction being matched
        vec![json!({"id": "tx-1", "transaction_type": "debit", "amount": "-500.000000"})],
        // the transaction, written back with the rule that took it
        vec![json!({"id": "tx-1", "matched_rule_id": "rule-1"})],
    ]);
    let (status, body) = send(graph, "/bank-rules/apply", json!(["tx-1"])).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["processed"], json!(1));
    assert_eq!(body["results"][0]["matched"], json!(true));
    assert_eq!(body["results"][0]["rule_id"], json!("rule-1"));
}

#[tokio::test]
async fn a_sync_is_recorded_and_the_connection_marked_as_syncing() {
    let graph = Scripted::queue(vec![
        vec![json!({"id": "conn-1", "provider": "plaid"})],
        vec![json!({"id": "sync-1", "status": "syncing"})],
        vec![json!({"id": "conn-1", "last_sync_status": "syncing"})],
    ]);
    let (status, body) = send(
        graph,
        "/sync/start",
        json!({"bank_connection_id": "conn-1", "start_date": "2026-02-01T00:00:00"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["sync_id"], json!("sync-1"));
    assert_eq!(body["status"], json!("started"));
}

#[tokio::test]
async fn a_connections_balance_says_the_figure_is_a_placeholder() {
    let graph = Scripted::queue(vec![vec![json!({"id": "conn-1", "currency": "GBP"})]]);
    let (status, body) = call(graph, Method::GET, "/connections/conn-1/balance", None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["source"], json!("placeholder"));
    assert_eq!(body["currency"], json!("GBP"));
    assert_eq!(body["current_balance"], json!("10500"));
}

#[tokio::test]
async fn the_statistics_of_an_organisations_connections_are_counted() {
    let graph = Scripted::queue(vec![
        vec![json!({"id": "conn-1", "provider": "plaid", "organization_id": "org-1"})],
        vec![
            json!({"id": "tx-1", "bank_connection_id": "conn-1", "status": "reconciled"}),
            json!({"id": "tx-2", "bank_connection_id": "conn-1", "status": "pending"}),
        ],
    ]);
    let (status, body) = call(
        graph,
        Method::GET,
        "/statistics?organization_id=org-1",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["total_connections"], json!(1));
    assert_eq!(body["total_transactions"], json!(2));
    assert_eq!(body["reconciliation_rate"], json!("50.000000"));
}

#[tokio::test]
async fn a_connection_that_is_not_the_books_is_a_miss() {
    let graph = Scripted::queue(vec![vec![]]);
    let (status, _) = call(graph, Method::GET, "/connections/conn-9", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
