//! webhook-service over HTTP: an endpoint registered, an event dispatched, and the trail of attempts.

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

    async fn get(&self, uri: &str) -> (StatusCode, Value) {
        let request = Request::builder()
            .method(Method::GET)
            .uri(uri)
            .header("x-user-id", "user-1")
            .header("x-book-id", "book-1")
            .body(Body::empty())
            .unwrap();
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

#[tokio::test]
async fn an_endpoint_subscribes_and_a_dispatch_records_how_it_went() {
    let book = Book::new();
    let (status, made) = book
        .call(
            Method::POST,
            "/endpoints",
            json!({
                "company_id": "co-1",
                // an https endpoint cannot be reached from this build, and the attempt is recorded rather than
                // pretended
                "url": "https://hooks.example.test/invoices",
                "secret": "s3cret",
                "events": ["invoice.created"],
                "created_at": "2026-01-01T09:00:00Z"
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{made}");
    // the secret is never read back
    assert!(made.get("secret").is_none());
    let endpoint_id = made["id"].as_str().expect("an id").to_string();

    let (status, listed) = book.get("/endpoints/co-1").await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    assert_eq!(listed.as_array().map(|held| held.len()), Some(1));
    assert_eq!(listed[0]["events"], json!(["invoice.created"]));

    let (status, sent) = book
        .call(
            Method::POST,
            "/dispatch/co-1",
            json!({
                "event_type": "invoice.created",
                "payload": {"id": "INV-1"},
                "now": "2026-01-02T09:00:00Z"
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{sent}");
    assert_eq!(sent["total_sent"], json!(1));
    assert_eq!(sent["successful"], json!(0));
    assert_eq!(sent["deliveries"][0]["status"], json!("failed"));
    assert_eq!(sent["deliveries"][0]["attempts"], json!(1));
    assert!(sent["deliveries"][0]["last_error"]
        .as_str()
        .unwrap_or_default()
        .contains("https"));

    // an event nobody subscribed to reaches nobody, and an unnamed event is refused
    let (status, none) = book
        .call(
            Method::POST,
            "/dispatch/co-1",
            json!({"event_type": "payment.received", "payload": {}}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{none}");
    assert_eq!(none["total_sent"], json!(0));

    let (status, _) = book
        .call(Method::POST, "/dispatch/co-1", json!({"payload": {}}))
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    let (status, trail) = book.get(&format!("/deliveries/{endpoint_id}")).await;
    assert_eq!(status, StatusCode::OK, "{trail}");
    assert_eq!(trail.as_array().map(|held| held.len()), Some(1));
    assert_eq!(trail[0]["event_type"], json!("invoice.created"));

    // a registration with no address is refused
    let (status, _) = book
        .call(Method::POST, "/endpoints", json!({"company_id": "co-1"}))
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}
