#![forbid(unsafe_code)]
//! banking over HTTP: statements, the cash book, and the reconciliations between them.

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
    let request = request.body(body).unwrap();
    let response = router.oneshot(request).await.unwrap();
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
async fn a_statement_is_imported_with_its_own_totals() {
    let graph = Scripted::queue(vec![vec![json!({"id": "s-1"})], vec![json!({"id": "s-1"})]]);
    let (status, body) = send(
        graph.clone(),
        "/statements",
        json!({
            "bank_account": "1234",
            "statement_number": "ST-1",
            "statement_start_date": "2026-03-01T00:00:00",
            "statement_end_date": "2026-03-31T00:00:00",
            "opening_balance": 100,
            "closing_balance": 850,
            "lines": [
                {"date": "2026-03-02", "description": "in", "reference": "A", "transaction_type": "deposit", "amount": 500},
                {"date": "2026-03-03", "description": "out", "reference": "B", "transaction_type": "cheque", "amount": 200, "is_debit": true},
            ],
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["total_credits"], json!("500.000000"));
    assert_eq!(body["total_debits"], json!("200.000000"));
}

#[tokio::test]
async fn a_line_of_an_unknown_kind_is_refused_over_http() {
    let graph = Scripted::queue(vec![]);
    let (status, body) = send(
        graph,
        "/statements",
        json!({
            "bank_account": "1234",
            "statement_number": "ST-1",
            "lines": [{"date": "2026-03-02", "description": "in", "transaction_type": "telepathy", "amount": 5}],
        }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
}

#[tokio::test]
async fn a_cash_book_entry_is_listed_by_its_account() {
    let graph = Scripted::queue(vec![vec![json!({
        "id": "e-1", "date": "2026-03-01", "matched": false,
        "amount": "500.000000", "description": "receipt", "reference": "A"
    })]]);
    let (status, body) = call(
        graph,
        Method::GET,
        "/cash-book?bank_account=1234&matched=false",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["entries"].as_array().unwrap().len(), 1);

    let graph = Scripted::queue(vec![]);
    let (status, body) = call(graph, Method::GET, "/cash-book", None).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
}

#[tokio::test]
async fn a_reconciliation_is_struck_from_the_query_it_is_sent_in() {
    let graph = Scripted::queue(vec![vec![json!({"id": "rec-1"})]]);
    let (status, body) = send(
        graph.clone(),
        "/bank-reconcile?bank_account=1234&reconciliation_date=2026-03-31T00:00:00&statement_balance=1000&cash_book_balance=900",
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["difference"], json!("100.000000"));
    assert_eq!(body["status"], json!("in_progress"));
}

#[tokio::test]
async fn a_reconciliation_answers_missing_when_it_does_not_belong_to_the_caller() {
    let graph = Scripted::queue(vec![vec![]]);
    let (status, _) = call(graph, Method::GET, "/reconciliations/rec-9", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn matching_a_statement_without_naming_it_is_refused() {
    let graph = Scripted::queue(vec![]);
    let (status, body) = send(graph, "/bank-reconcile/rec-1/auto-match", json!({})).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
}

#[tokio::test]
async fn a_reconciliation_with_nothing_to_adjust_completes_without_a_journal() {
    let graph = Scripted::queue(vec![
        // the reconciliation, with no bank error to adjust
        vec![json!({
            "id": "rec-1", "bank_account": "1234", "bank_errors": "0",
            "reconciliation_date": "2026-03-31T00:00:00", "status": "in_progress"
        })],
        // the reconciliation, completed
        vec![json!({"id": "rec-1", "status": "completed", "journal_entry_id": null})],
    ]);
    let (status, body) = send(
        graph.clone(),
        "/bank-reconcile/rec-1/post-adjustments",
        json!({"today": "2026-04-01T00:00:00"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["status"], json!("completed"));
    assert_eq!(body["journal_entry_id"], Value::Null);
}

#[tokio::test]
async fn a_bare_bank_error_is_not_posted_as_a_one_sided_entry() {
    let graph = Scripted::queue(vec![vec![json!({
        "id": "rec-1", "bank_account": "1234", "bank_errors": "25.000000",
        "reconciliation_date": "2026-03-31T00:00:00", "status": "in_progress"
    })]]);
    let (status, body) = send(graph, "/bank-reconcile/rec-1/post-adjustments", json!({})).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["error"], json!("an entry needs at least two lines"));
}

#[tokio::test]
async fn outstanding_items_need_the_account_they_are_asked_about() {
    let graph = Scripted::queue(vec![]);
    let (status, body) = call(graph, Method::GET, "/outstanding-items", None).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");

    let graph = Scripted::queue(vec![vec![json!({
        "id": "rec-1", "bank_account": "1234", "outstanding_cheques": "150.000000",
        "outstanding_deposits": "50.000000",
        "items": [{"date": "2026-03-01", "description": "Cheque 1042", "amount": "150.000000", "reference": "1042"}]
    })]]);
    let (status, body) = call(
        graph,
        Method::GET,
        "/outstanding-items?bank_account=1234",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["outstanding_cheques"].as_array().unwrap().len(), 1);
    assert_eq!(body["total_cheques"], json!("150.000000"));
}

#[tokio::test]
async fn the_reconciliation_listing_narrows_by_state() {
    let graph = Scripted::queue(vec![vec![
        json!({"id": "rec-1", "bank_account": "1234", "status": "completed"}),
        json!({"id": "rec-2", "bank_account": "1234", "status": "in_progress"}),
    ]]);
    let (status, body) = call(
        graph,
        Method::GET,
        "/reconciliations?status=in_progress",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["reconciliations"].as_array().unwrap().len(), 1);
    assert_eq!(body["reconciliations"][0]["id"], json!("rec-2"));
}
