#![forbid(unsafe_code)]
//! cache-service over HTTP: a value held under a key, its life, and the cache's own counts.

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
async fn a_value_held_for_a_while_and_then_gone() {
    let book = Book::new();
    let (status, written) = book
        .call(
            Method::POST,
            "/cache",
            json!({"key": "rates", "value": {"usd": 1.25}, "now": 1000, "ttl": 60}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{written}");
    assert_eq!(written["found"], json!(true));
    assert_eq!(written["ttl_remaining"], json!(60));

    let (status, hit) = book.get("/cache/rates?now=1030").await;
    assert_eq!(status, StatusCode::OK, "{hit}");
    assert_eq!(hit["value"], json!({"usd": 1.25}));
    assert_eq!(hit["ttl_remaining"], json!(30));

    // once its life is over the read is a miss, and the stale value is not handed back
    let (status, stale) = book.get("/cache/rates?now=1061").await;
    assert_eq!(status, StatusCode::OK, "{stale}");
    assert_eq!(stale["found"], json!(false));

    // a read with no clock is refused rather than guessed at
    let (status, _) = book.get("/cache/rates").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    let (status, gone) = book.call(Method::DELETE, "/cache/absent", json!({})).await;
    assert_eq!(status, StatusCode::OK, "{gone}");
    assert_eq!(gone["deleted"], json!(false));

    let (status, counted) = book.get("/cache-stats?now=1061").await;
    assert_eq!(status, StatusCode::OK, "{counted}");
    assert_eq!(counted["stats"]["hits"], json!(1));
    assert_eq!(counted["stats"]["misses"], json!(1));
    assert_eq!(counted["cache_size"], json!(0));

    let (status, cleared) = book.call(Method::DELETE, "/cache", json!({})).await;
    assert_eq!(status, StatusCode::OK, "{cleared}");
    assert_eq!(cleared["cleared"], json!(0));
}
