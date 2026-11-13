//! bank relationships, the quality of a bank's service, and the plain connection door, over HTTP.

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
async fn a_relationship_is_recorded_against_its_company() {
    let graph = Scripted::queue(vec![vec![json!({
        "id": "r-1", "company_id": "co-1", "bank_name": "CBZ", "status": "active", "rating": 3
    })]]);
    let (status, body) = send(
        graph,
        "/relationships",
        json!({"company_id": "co-1", "bank_name": "CBZ", "services": ["payments"]}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["bank_name"], json!("CBZ"));
    // a relationship nobody rated starts at 3
    assert_eq!(body["rating"], json!(3));
}

#[tokio::test]
async fn a_relationship_cannot_be_rated_outside_one_to_five() {
    let graph = Scripted::queue(vec![]);
    let (status, body) = send(
        graph,
        "/relationships",
        json!({"company_id": "co-1", "bank_name": "CBZ", "rating": 9}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(
        body["error"],
        json!("rating must be between 1 and 5, not 9")
    );
}

#[tokio::test]
async fn a_companys_relationships_are_listed_by_status() {
    let graph = Scripted::queue(vec![vec![
        json!({"id": "r-1", "company_id": "co-1", "status": "active", "bank_name": "CBZ"}),
        json!({"id": "r-2", "company_id": "co-1", "status": "dormant", "bank_name": "Stanbic"}),
        json!({"id": "r-3", "company_id": "co-9", "status": "active", "bank_name": "NMB"}),
    ]]);
    let (status, body) = call(
        graph,
        Method::GET,
        "/relationships/co-1?status_filter=active",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["total"], json!(1));
    assert_eq!(body["relationships"][0]["id"], json!("r-1"));
}

#[tokio::test]
async fn a_relationships_quality_is_summarised_with_its_metrics() {
    let graph = Scripted::queue(vec![
        vec![json!({"id": "r-1", "company_id": "co-1"})],
        vec![
            json!({"id": "m-1", "relationship_id": "r-1", "score": 2, "recorded_at": "2026-03-01"}),
            json!({"id": "m-2", "relationship_id": "r-1", "score": 5, "recorded_at": "2026-03-02"}),
        ],
    ]);
    let (status, body) = call(graph, Method::GET, "/quality-metrics/r-1", None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["avg_score"], json!(3.5));
    assert_eq!(body["metrics"].as_array().expect("a list").len(), 2);
}

#[tokio::test]
async fn the_summary_of_a_relationship_book_counts_banks_and_services_once() {
    let graph = Scripted::queue(vec![vec![
        json!({"id": "r-1", "company_id": "co-1", "bank_name": "CBZ", "status": "active",
               "rating": 4, "services": ["payments", "loans"]}),
        json!({"id": "r-2", "company_id": "co-1", "bank_name": "CBZ", "status": "active",
               "rating": 2, "services": ["loans"]}),
    ]]);
    let (status, body) = call(graph, Method::GET, "/bank-summary/co-1", None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["unique_banks"], json!(1));
    assert_eq!(body["total_services"], json!(2));
    assert_eq!(body["avg_rating"], json!(3.0));
}

#[tokio::test]
async fn a_connection_to_an_account_already_connected_is_a_duplicate() {
    let graph = Scripted::queue(vec![vec![json!({
        "id": "conn-1", "bank_name": "CBZ", "account_number": "1234"
    })]]);
    let (status, body) = send(
        graph,
        "/connect",
        json!({"bank_name": "CBZ", "account_number": "1234"}),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(
        body["error"],
        json!("Connection to CBZ account 1234 already exists")
    );
}

#[tokio::test]
async fn a_sync_of_a_disconnected_account_is_refused() {
    let graph = Scripted::queue(vec![vec![
        json!({"id": "conn-1", "status": "disconnected"}),
    ]]);
    let (status, body) = send(graph, "/bank-sync/conn-1", json!({})).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["error"], json!("Connection is not active"));
}

#[tokio::test]
async fn a_transaction_is_reconciled_through_its_own_connection() {
    let graph = Scripted::queue(vec![
        vec![json!({"id": "tx-1", "bank_connection_id": "conn-1", "status": "cleared"})],
        vec![json!({"id": "tx-1", "bank_connection_id": "conn-1", "status": "reconciled"})],
    ]);
    let (status, body) = send(
        graph,
        "/bank-reconcile/conn-1",
        json!({"transaction_id": "tx-1", "matched_entry_id": "je-1"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["reconciled"], json!(true));
    assert_eq!(body["matched_entry"], json!("je-1"));
}

#[tokio::test]
async fn a_connections_transactions_are_paged() {
    let graph = Scripted::queue(vec![
        vec![json!({"id": "conn-1", "status": "active"})],
        vec![
            json!({"id": "tx-1", "bank_connection_id": "conn-1", "date": "2026-03-03"}),
            json!({"id": "tx-2", "bank_connection_id": "conn-1", "date": "2026-03-02"}),
            json!({"id": "tx-3", "bank_connection_id": "conn-1", "date": "2026-03-01"}),
        ],
    ]);
    let (status, body) = call(
        graph,
        Method::GET,
        "/bank-connection-transactions/conn-1?limit=2",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body.as_array().expect("a list").len(), 2);
    assert_eq!(body[0]["id"], json!("tx-1"));
}

#[tokio::test]
async fn a_connection_that_is_not_the_books_is_a_miss() {
    let graph = Scripted::queue(vec![vec![]]);
    let (status, _) = send(graph, "/bank-sync/conn-9", json!({})).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
