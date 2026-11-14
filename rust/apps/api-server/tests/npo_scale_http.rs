#![forbid(unsafe_code)]
//! npo-scale-service over HTTP: the organisation register, credit purchases, and the creditors report.

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

async fn call(graph: Arc<Scripted>, method: Method, uri: &str) -> (StatusCode, Value) {
    call_with(graph, method, uri, None, true).await
}

async fn send(graph: Arc<Scripted>, method: Method, uri: &str, body: Value) -> (StatusCode, Value) {
    call_with(graph, method, uri, Some(body), true).await
}

/// The public routes ask for no headers, so they are called without a Book.
async fn call_public(graph: Arc<Scripted>, uri: &str) -> (StatusCode, Value) {
    call_with(graph, Method::GET, uri, None, false).await
}

async fn call_with(
    graph: Arc<Scripted>,
    method: Method,
    uri: &str,
    payload: Option<Value>,
    with_book: bool,
) -> (StatusCode, Value) {
    let router = vimbai_api::records::router(graph);
    let mut request = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json");
    if with_book {
        request = request
            .header("x-user-id", "user-1")
            .header("x-book-id", "book-1");
    }
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

#[tokio::test]
async fn an_organisation_is_created_at_the_band_its_revenue_earns() {
    let graph = Scripted::queue(vec![vec![json!({"id": "org-1"})]]);
    let (status, body) = call(
        graph.clone(),
        Method::POST,
        "/npo-scale/orgs?name=Harare%20Youth%20Trust&org_type=nonprofit&annual_revenue=60000",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let (cypher, params) = graph.last();
    assert!(cypher.contains("NPOOrg"), "{cypher}");
    assert_eq!(params["props"]["size_band"], json!("medium"));
    assert_eq!(params["props"]["owner_id"], json!("user-1"));
}

#[tokio::test]
async fn another_tenants_organisation_is_a_miss() {
    let graph = Scripted::queue(vec![vec![json!({"id": "org-1", "owner_id": "user-2"})]]);
    let (status, _) = call(graph, Method::GET, "/npo-scale/orgs/org-1").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn the_features_carry_the_approval_limit_of_the_band() {
    let graph = Scripted::queue(vec![vec![json!({
        "id": "org-1",
        "owner_id": "user-1",
        "org_type": "nonprofit",
        "size_band": "medium",
    })]]);
    let (status, body) = call(graph, Method::GET, "/npo-scale/orgs/org-1/features").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["approval_limit"], json!(5000));
    assert!(body["features"]
        .as_array()
        .unwrap()
        .iter()
        .any(|f| f == "donor_crm"));
}

#[tokio::test]
async fn creditors_are_what_is_still_owed() {
    let graph = Scripted::queue(vec![
        vec![
            json!({"id": "org-1", "owner_id": "user-1", "org_type": "nonprofit", "size_band": "small"}),
        ],
        vec![json!({"id": "v-1", "org_id": "org-1", "name": "Stationery"})],
        vec![
            json!({"id": "p-1", "org_id": "org-1", "vendor_id": "v-1", "status": "unpaid", "amount": "120.00"}),
            json!({"id": "p-2", "org_id": "org-1", "vendor_id": "v-1", "status": "paid", "amount": "80.00"}),
        ],
    ]);
    let (status, body) = call(
        graph,
        Method::GET,
        "/npo-scale/orgs/org-1/reports/creditors",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["total_owed"], json!("120.000000"));
    assert_eq!(body["creditors"][0]["vendor"], json!("Stationery"));
}

#[tokio::test]
async fn a_purchase_paid_twice_is_refused() {
    let graph = Scripted::queue(vec![
        vec![
            json!({"id": "org-1", "owner_id": "user-1", "org_type": "nonprofit", "size_band": "small"}),
        ],
        vec![json!({"id": "p-1", "org_id": "org-1", "status": "paid", "amount": "120.00"})],
    ]);
    let (status, body) = call(
        graph,
        Method::POST,
        "/npo-scale/orgs/org-1/purchases/p-1/pay",
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(body["error"].as_str().unwrap().contains("Already paid"));
}

#[tokio::test]
async fn a_purchase_with_no_amount_is_refused() {
    let graph = Scripted::queue(vec![vec![json!({
        "id": "org-1",
        "owner_id": "user-1",
        "org_type": "nonprofit",
        "size_band": "small",
    })]]);
    let (status, body) = call(
        graph,
        Method::POST,
        "/npo-scale/orgs/org-1/purchases?vendor_id=v-1&amount=0",
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(body["error"].as_str().unwrap().contains("positive"));
}

#[tokio::test]
async fn a_donation_answers_with_the_receipt_it_issued() {
    let graph = Scripted::queue(vec![
        vec![
            json!({"id": "org-1", "owner_id": "user-1", "org_type": "nonprofit", "size_band": "small"}),
        ],
        vec![json!({"id": "donor-1", "org_id": "org-1", "name": "Tendai"})],
        vec![json!({"id": "donation-1", "org_id": "org-1", "amount": "250.000000"})],
        // no receipts yet, so this is the organisation's first
        Vec::new(),
        vec![json!({"id": "receipt-1", "receipt_no": "RCP-org-1-00001", "token": "abc123"})],
        vec![json!({"id": "donation-1", "receipt_id": "receipt-1"})],
    ]);
    let (status, body) = send(
        graph,
        Method::POST,
        "/npo-scale/orgs/org-1/donations",
        json!({"donor_id": "donor-1", "amount": 250}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["receipt"]["receipt_no"], json!("RCP-org-1-00001"));
    assert_eq!(body["receipt"]["token"], json!("abc123"));
}

#[tokio::test]
async fn a_receipt_can_be_checked_by_anyone_holding_its_token() {
    let graph = Scripted::queue(vec![
        vec![json!({
            "id": "receipt-1",
            "org_id": "org-1",
            "user_id": "user-1",
            "book_id": "book-1",
            "receipt_no": "RCP-org-1-00001",
            "amount": "250.000000",
            "currency": "USD",
            "issued_at": "2026-03-01",
            "token": "abc123",
        })],
        vec![json!({"id": "org-1", "name": "Harare Youth Trust"})],
    ]);
    let (status, body) = call_public(graph, "/npo-scale/receipts/verify/abc123").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["valid"], json!(true));
    assert_eq!(body["receipt"]["receipt_no"], json!("RCP-org-1-00001"));
    assert_eq!(body["receipt"]["name"], json!("Harare Youth Trust"));
}

#[tokio::test]
async fn an_unknown_token_is_not_a_receipt() {
    let graph = Scripted::queue(vec![Vec::new()]);
    let (status, body) = call_public(graph, "/npo-scale/receipts/verify/nothing").await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
}

#[tokio::test]
async fn a_receipt_downloads_as_a_pdf() {
    let graph = Scripted::queue(vec![
        vec![
            json!({"id": "org-1", "owner_id": "user-1", "name": "Harare Youth Trust", "org_type": "nonprofit"}),
        ],
        vec![json!({
            "id": "receipt-1",
            "org_id": "org-1",
            "receipt_no": "RCP-org-1-00001",
            "amount": "250.000000",
            "currency": "USD",
            "issued_at": "2026-03-01",
            "token": "abc123",
        })],
    ]);
    let router = vimbai_api::records::router(graph);
    let request = Request::builder()
        .method(Method::GET)
        .uri("/npo-scale/orgs/org-1/receipts/receipt-1/pdf")
        .header("x-user-id", "user-1")
        .header("x-book-id", "book-1")
        .body(Body::empty())
        .unwrap();
    let response = router.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()["content-type"],
        "application/pdf",
        "not served as a document"
    );
    let bytes = axum::body::to_bytes(response.into_body(), 1_000_000)
        .await
        .unwrap();
    assert!(String::from_utf8_lossy(&bytes).starts_with("%PDF-1.4"));
}

#[tokio::test]
async fn an_investor_checks_their_own_holding_without_the_books() {
    let graph = Scripted::queue(vec![
        vec![json!({
            "id": "s-1",
            "org_id": "org-1",
            "user_id": "user-1",
            "book_id": "book-1",
            "name": "Rudo",
            "shares": 250,
            "share_class": "ordinary",
            "amount_paid": "2500.000000",
            "verify_code": "hold99",
        })],
        vec![json!({"id": "org-1", "name": "Kaguvi Holdings"})],
        vec![
            json!({"id": "s-1", "org_id": "org-1", "shares": 250}),
            json!({"id": "s-2", "org_id": "org-1", "shares": 750}),
        ],
        vec![json!({"id": "d-1", "org_id": "org-1", "per_share": "0.400000"})],
    ]);
    let (status, body) = call_public(graph, "/npo-scale/public/holdings/hold99").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["org_name"], json!("Kaguvi Holdings"));
    assert_eq!(body["percentage"], json!("25.000000"));
    assert_eq!(body["total_dividends"], json!("100.000000"));
}

#[tokio::test]
async fn a_pledge_run_collects_nothing_until_a_pledge_is_due() {
    let graph = Scripted::queue(vec![
        vec![
            json!({"id": "org-1", "owner_id": "user-1", "org_type": "nonprofit", "size_band": "small"}),
        ],
        vec![
            json!({"id": "p-1", "org_id": "org-1", "status": "active", "next_due": "2026-04-01", "amount": "50.000000"}),
        ],
    ]);
    let (status, body) = send(
        graph,
        Method::POST,
        "/npo-scale/orgs/org-1/pledges/run?today=2026-03-01",
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["collected"], json!(0));
}

#[tokio::test]
async fn the_consolidated_report_is_refused_below_large_scale() {
    let graph = Scripted::queue(vec![vec![json!({
        "id": "org-1",
        "owner_id": "user-1",
        "org_type": "nonprofit",
        "size_band": "medium",
    })]]);
    let (status, body) = call(
        graph,
        Method::GET,
        "/npo-scale/orgs/org-1/reports/consolidated",
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(body["error"].as_str().unwrap().contains("large scale"));
}
