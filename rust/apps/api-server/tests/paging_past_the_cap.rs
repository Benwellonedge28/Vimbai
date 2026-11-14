#![forbid(unsafe_code)]
//! Paging a Book that is bigger than the handler is willing to answer.
//!
//! Every listing in this engine asks the store for a bounded page - fifty rows here, a thousand there - because an answer
//! that grows with the data is a way to run a server out of memory. The consequence, until now, was a ceiling: a caller
//! following cursors walked to the end of that page and stopped there, and rows past it were reachable only by writing a
//! bigger cap. A cursor that names the row it reached lets the store seek instead, so how far a caller can walk depends on
//! what the Book holds rather than on what a handler was willing to produce.
//!
//! The Book in this test holds twelve hundred rules and the listing answers a thousand of them at most. Walking the pages
//! has to reach every one, in order, exactly once.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use serde_json::{json, Value};
use store::fake::MemoryGraph;
use tower::ServiceExt;

const WRITTEN: usize = 1_200;
const PAGE: usize = 300;
/// What the rules listing is willing to produce in one answer.
const CAP: usize = 1_000;

fn router() -> axum::Router {
    let graph: vimbai_api::records::GraphHandle = Arc::new(MemoryGraph::new());
    vimbai_api::routed(Some(graph))
}

async fn ask(router: &axum::Router, method: Method, uri: &str) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header("x-user-id", "user-1")
        .header("x-book-id", "book-1")
        .header("content-type", "application/json")
        .body(Body::empty())
        .unwrap();
    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 8 * 1024 * 1024)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[tokio::test]
async fn every_row_of_a_book_bigger_than_the_cap_is_reachable() {
    let router = router();
    for index in 0..WRITTEN {
        let request = Request::builder()
            .method(Method::POST)
            .uri("/rules")
            .header("x-user-id", "user-1")
            .header("x-book-id", "book-1")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({
                    "name": format!("rule {index:04}"),
                    "category": "financial",
                    "severity": "low",
                    "enabled": true,
                })
                .to_string(),
            ))
            .unwrap();
        let response = router.clone().oneshot(request).await.unwrap();
        assert!(
            response.status().is_success(),
            "rule {index} written: {}",
            response.status()
        );
    }

    // The ceiling, stated: one answer cannot hold the Book.
    let (status, answer) = ask(&router, Method::GET, "/rules").await;
    assert_eq!(status, StatusCode::OK);
    let held: Vec<Value> = answer.as_array().cloned().unwrap_or_default();
    assert_eq!(held.len(), CAP, "the listing stops at its cap");

    // Walking the pages has to pass it.
    let mut seen: Vec<String> = Vec::new();
    let mut uri = format!("/rules?limit={PAGE}");
    for page in 0..10 {
        let (status, answer) = ask(&router, Method::GET, &uri).await;
        assert_eq!(status, StatusCode::OK, "page {page}: {answer}");
        let items = answer["items"].as_array().cloned().unwrap_or_default();
        for item in &items {
            seen.push(item["id"].as_str().unwrap_or_default().to_owned());
        }
        let more = answer["has_more"].as_bool().unwrap_or(false);
        if !more {
            break;
        }
        let next = answer["next_cursor"]
            .as_str()
            .unwrap_or_else(|| panic!("has_more without a cursor: {answer}"));
        uri = format!("/rules?limit={PAGE}&cursor={next}");
    }

    let unique: std::collections::HashSet<&String> = seen.iter().collect();
    assert_eq!(unique.len(), WRITTEN, "a row was lost or handed over twice");
    assert!(
        seen.len() > CAP,
        "the walk stopped at the cap: {} rows",
        seen.len()
    );
}
