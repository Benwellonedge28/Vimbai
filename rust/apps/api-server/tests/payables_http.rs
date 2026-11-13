//! payables over HTTP: the purchases ledger and its control account, and the expenses claimed against it.

use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use serde_json::{json, Value};
use store::{Graph, StoreError};
use tower::ServiceExt;

struct Scripted {
    queue: Mutex<Vec<Vec<Value>>>,
    seen: Mutex<Vec<(String, Value)>>,
}

impl Scripted {
    fn queue(scripts: Vec<Vec<Value>>) -> Arc<Self> {
        Arc::new(Self {
            queue: Mutex::new(scripts),
            seen: Mutex::new(Vec::new()),
        })
    }

    /// The statement that was run last, with the parameters it was run with.
    fn last(&self) -> (String, Value) {
        self.seen
            .lock()
            .unwrap()
            .last()
            .cloned()
            .unwrap_or_else(|| (String::new(), Value::Null))
    }
}

impl Graph for Scripted {
    fn run(&self, cypher: &str, params: Value) -> Result<Vec<Value>, StoreError> {
        self.seen.lock().unwrap().push((cypher.to_owned(), params));
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
async fn a_creditor_transaction_is_stamped_with_the_balance_it_leaves() {
    let graph = Scripted::queue(vec![
        vec![json!({
            "id": "t-1", "creditor_id": "c-1", "transaction_type": "purchase_invoice",
            "amount": "1000.000000", "date": "2026-01-01"
        })],
        vec![json!({"id": "t-2"})],
    ]);
    let (status, body) = send(
        graph.clone(),
        "/creditor-transactions",
        json!({
            "creditor_id": "c-1",
            "creditor_name": "Harare Wholesale",
            "transaction_type": "purchase_invoice",
            "amount": 250,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["id"], json!("t-2"));
    assert_eq!(body["balance"], json!("1250.000000"));
    let (_, params) = graph.last();
    assert_eq!(params["props"]["creditor_name"], json!("Harare Wholesale"));
}

#[tokio::test]
async fn a_creditor_transaction_without_a_creditor_is_refused() {
    let graph = Scripted::queue(vec![Vec::new()]);
    let (status, _) = send(
        graph,
        "/creditor-transactions",
        json!({"transaction_type": "payment", "amount": 100}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn the_purchases_control_account_answers_its_movements() {
    let graph = Scripted::queue(vec![vec![
        json!({"id": "t-1", "creditor_id": "c-1", "transaction_type": "purchase_invoice", "amount": "1000.000000", "date": "2026-01-01"}),
        json!({"id": "t-2", "creditor_id": "c-1", "transaction_type": "payment", "amount": "400.000000", "date": "2026-01-10"}),
    ]]);
    let (status, body) = call(
        graph,
        Method::GET,
        "/purchases-control-account/summary",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["total_invoices"], json!("1000.000000"));
    assert_eq!(body["total_payments"], json!("400.000000"));
    assert_eq!(body["closing_balance"], json!("600.000000"));
}

#[tokio::test]
async fn the_creditors_are_listed_with_their_balances() {
    let graph = Scripted::queue(vec![vec![
        json!({"id": "t-1", "creditor_id": "c-1", "transaction_type": "purchase_invoice", "amount": "1000.000000", "date": "2026-01-01"}),
        json!({"id": "t-2", "creditor_id": "c-2", "transaction_type": "purchase_invoice", "amount": "250.000000", "date": "2026-01-02"}),
    ]]);
    let (status, body) = call(graph, Method::GET, "/creditors", None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["creditors"].as_array().unwrap().len(), 2);
    assert_eq!(body["total_balance"], json!("1250.000000"));
}

#[tokio::test]
async fn the_purchases_ledger_reconciles_with_its_control_account() {
    let graph = Scripted::queue(vec![
        vec![
            json!({"id": "t-1", "creditor_id": "c-1", "transaction_type": "purchase_invoice", "amount": "1000.000000", "date": "2026-01-01"}),
            json!({"id": "t-2", "creditor_id": "c-1", "transaction_type": "debit_note", "amount": "100.000000", "date": "2026-01-05"}),
        ],
        vec![
            json!({"id": "t-1", "creditor_id": "c-1", "transaction_type": "purchase_invoice", "amount": "1000.000000", "date": "2026-01-01"}),
            json!({"id": "t-2", "creditor_id": "c-1", "transaction_type": "debit_note", "amount": "100.000000", "date": "2026-01-05"}),
        ],
    ]);
    let (status, body) = call(graph, Method::POST, "/purchases-reconcile", None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["difference"], json!("0.000000"));
    assert_eq!(body["reconciled"], json!(true));
}

#[tokio::test]
async fn an_expense_arrives_pending() {
    let graph = Scripted::queue(vec![vec![json!({"id": "e-1"})]]);
    let (status, body) = send(
        graph.clone(),
        "/expenses",
        json!({
            "company_id": "co-1",
            "employee_id": "emp-1",
            "category": "travel",
            "amount": 240.5,
            "status": "approved",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["status"], json!("pending"));
    let (_, params) = graph.last();
    assert_eq!(params["props"]["status"], json!("pending"));
}

#[tokio::test]
async fn an_expense_of_nothing_is_refused() {
    let graph = Scripted::queue(vec![Vec::new()]);
    let (status, _) = send(
        graph,
        "/expenses",
        json!({"company_id": "co-1", "employee_id": "emp-1", "category": "meals", "amount": 0}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn the_expenses_are_listed_for_a_company_and_narrowed_by_state() {
    let graph = Scripted::queue(vec![vec![
        json!({"id": "e-1", "company_id": "co-1", "category": "travel", "amount": "240.500000", "status": "approved"}),
        json!({"id": "e-2", "company_id": "co-1", "category": "travel", "amount": "100.000000", "status": "pending"}),
    ]]);
    let (status, body) = call(graph, Method::GET, "/expenses/co-1?status=pending", None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["company_id"], json!("co-1"));
    assert_eq!(body["total"], json!(1));
    assert_eq!(body["expenses"][0]["id"], json!("e-2"));
}

#[tokio::test]
async fn approving_an_expense_records_who_approved_it() {
    let graph = Scripted::queue(vec![vec![json!({
        "id": "e-1", "company_id": "co-1", "category": "travel", "amount": "240.500000",
        "status": "approved", "approved_by": "manager-1"
    })]]);
    let (status, body) = call(
        graph.clone(),
        Method::PUT,
        "/expenses/e-1/approve?approver=manager-1",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["status"], json!("approved"));
    assert_eq!(body["approved_by"], json!("manager-1"));
    let (_, params) = graph.last();
    assert_eq!(params["patch"]["status"], json!("approved"));
}

#[tokio::test]
async fn rejecting_an_expense_records_why() {
    let graph = Scripted::queue(vec![vec![json!({
        "id": "e-1", "company_id": "co-1", "category": "meals", "amount": "40.000000",
        "status": "rejected", "rejection_reason": "no receipt"
    })]]);
    let (status, body) = call(
        graph.clone(),
        Method::PUT,
        "/expenses/e-1/reject?reason=no%20receipt",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["status"], json!("rejected"));
    assert_eq!(body["rejection_reason"], json!("no receipt"));
    let (_, params) = graph.last();
    assert_eq!(params["patch"]["rejection_reason"], json!("no receipt"));
}

#[tokio::test]
async fn an_expense_that_is_not_the_callers_answers_as_missing() {
    let graph = Scripted::queue(vec![Vec::new()]);
    let (status, _) = call(graph, Method::PUT, "/expenses/e-9/approve", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn the_expense_summary_adds_by_category_and_counts_by_state() {
    let graph = Scripted::queue(vec![vec![
        json!({"id": "e-1", "company_id": "co-1", "category": "travel", "amount": "240.500000", "status": "approved"}),
        json!({"id": "e-2", "company_id": "co-1", "category": "meals", "amount": "40.000000", "status": "pending"}),
    ]]);
    let (status, body) = call(graph, Method::GET, "/expense-summary/co-1", None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["total_expenses"], json!(2));
    assert_eq!(body["total_amount"], json!("280.500000"));
    assert_eq!(body["by_category"]["travel"], json!("240.500000"));
    assert_eq!(body["by_status"]["pending"], json!(1));
}
