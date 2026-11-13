//! subscription-plans-service over HTTP: a plan recorded, a subscription taken against it, and the upgrade quoted.

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use serde_json::{json, Value};
use std::sync::Arc;
use store::fake::MemoryGraph;
use tower::ServiceExt;

struct Book {
    router: axum::Router,
}

impl Book {
    fn new() -> Self {
        Self {
            router: vimbai_api::records::router(Arc::new(MemoryGraph::new())),
        }
    }

    async fn call(&self, method: Method, uri: &str, payload: Value) -> (StatusCode, Value) {
        let request = Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json")
            .header("x-user-id", "user-1")
            .header("x-book-id", "book-1")
            .body(Body::from(payload.to_string()))
            .unwrap();
        self.answer(request).await
    }

    async fn get(&self, uri: &str) -> (StatusCode, Value) {
        let request = Request::builder()
            .method(Method::GET)
            .uri(uri)
            .header("x-user-id", "user-1")
            .header("x-book-id", "book-1")
            .body(Body::empty())
            .unwrap();
        self.answer(request).await
    }

    async fn answer(&self, request: Request<Body>) -> (StatusCode, Value) {
        let response = self.router.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 1_000_000)
            .await
            .unwrap();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }
}

async fn a_plan(book: &Book) -> Value {
    let (status, made) = book
        .call(
            Method::POST,
            "/plans",
            json!({
                "tier": "professional",
                "name": "Professional",
                "price_monthly": 199,
                "features": ["ledgers", "reports"],
                "created_at": "2026-01-01T09:00:00Z"
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{made}");
    made
}

#[tokio::test]
async fn a_plan_is_recorded_and_listed() {
    let book = Book::new();
    let made = a_plan(&book).await;
    assert_eq!(made["tier"], json!("professional"));
    assert_eq!(made["max_users"], json!(5));

    let (status, listed) = book.get("/plans").await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    assert_eq!(listed.as_array().map(|held| held.len()), Some(1));
    assert_eq!(listed[0]["id"], made["id"]);
}

#[tokio::test]
async fn a_subscription_needs_the_callers_own_plan() {
    let book = Book::new();
    let plan = a_plan(&book).await;
    let plan_id = plan["id"].as_str().expect("an id");

    let (status, taken) = book
        .call(
            Method::POST,
            "/subscribe",
            json!({"company_id": "co-1", "plan_id": plan_id, "cycle": "annual", "today": "2026-03-01"}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{taken}");
    assert_eq!(taken["tier"], json!("professional"));
    assert_eq!(taken["current_period_end"], json!("2027-03-01"));

    let (status, listed) = book.get("/subscriptions/co-1").await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    assert_eq!(listed.as_array().map(|held| held.len()), Some(1));

    let (status, _) = book
        .call(
            Method::POST,
            "/subscribe",
            json!({"company_id": "co-1", "plan_id": "no-such-plan", "today": "2026-03-01"}),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn an_upgrade_is_quoted_without_reading_a_stored_plan() {
    let book = Book::new();
    a_plan(&book).await;
    // the quote prices the fixed table, so the plan just recorded changes nothing
    let (status, quoted) = book
        .call(
            Method::POST,
            "/upgrade",
            json!({
                "company_id": "co-1",
                "current_plan": "basic",
                "target_plan": "professional",
                "current_period_end": "2026-04-01",
                "today": "2026-03-02"
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{quoted}");
    assert_eq!(quoted["proration_amount"], json!(150.0));
    assert_eq!(quoted["new_billing_amount"], json!(199.0));
    assert_eq!(quoted["cycle"], json!("monthly"));
}
