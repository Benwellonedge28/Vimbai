#![forbid(unsafe_code)]
//! Two things a busy engine must do: hand a large page over as it is produced, and admit work at a rate it can finish.
//!
//! The first is a representation: `Accept: application/x-ndjson` turns a page into one record per line with the paging
//! facts in headers, so a caller on a slow link starts on the first row instead of waiting for the last. The second is the
//! door in `limits.rs`, which is exercised here through the records router rather than on a toy route.

use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt;

#[allow(unused_imports)]
use store::fake::MemoryGraph;

async fn call(
    graph: Arc<MemoryGraph>,
    method: Method,
    uri: &str,
    accept: &str,
    payload: Option<Value>,
) -> (StatusCode, axum::http::HeaderMap, String) {
    let router = vimbai_api::pagination::guard(vimbai_api::records::router(graph));
    let body = match payload {
        Some(value) => Body::from(value.to_string()),
        None => Body::empty(),
    };
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .header(header::ACCEPT, accept)
        .header("x-user-id", "user-1")
        .header("x-book-id", "book-1")
        .body(body)
        .unwrap();
    let response = router.oneshot(request).await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = axum::body::to_bytes(response.into_body(), 1_000_000)
        .await
        .unwrap();
    (
        status,
        headers,
        String::from_utf8_lossy(&bytes).into_owned(),
    )
}

fn a_rule(name: &str) -> Value {
    json!({
        "name": name,
        "category": "fraud",
        "severity": "critical",
        "condition": {"type": "threshold", "field": "amount", "operator": "gt", "value": 10000},
        "cooldown_seconds": 3600,
    })
}

async fn book_with(graph: &Arc<MemoryGraph>, count: usize) {
    for index in 0..count {
        let (status, _, body) = call(
            graph.clone(),
            Method::POST,
            "/rules",
            "application/json",
            Some(a_rule(&format!("Rule {index}"))),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
    }
}

#[tokio::test]
async fn a_page_can_be_read_one_record_at_a_time_with_its_paging_in_headers() {
    let graph = Arc::new(MemoryGraph::new());
    book_with(&graph, 5).await;

    let (status, headers, body) = call(
        graph.clone(),
        Method::GET,
        "/rules?limit=2",
        "application/x-ndjson",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        headers
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("application/x-ndjson")
    );
    assert_eq!(
        headers.get("x-total-count").and_then(|v| v.to_str().ok()),
        Some("5")
    );
    assert_eq!(
        headers.get("x-has-more").and_then(|v| v.to_str().ok()),
        Some("true")
    );
    assert_eq!(
        headers.get("x-page-count").and_then(|v| v.to_str().ok()),
        Some("2")
    );

    let lines: Vec<&str> = body.lines().filter(|line| !line.is_empty()).collect();
    assert_eq!(lines.len(), 2, "one line per record: {body}");
    for line in &lines {
        let record: Value = serde_json::from_str(line).expect("a line is a whole record");
        assert!(record["id"].is_string(), "{line}");
    }

    // the cursor in the header is the same one the envelope would carry, so the next page is readable the same way
    let cursor = headers
        .get("x-next-cursor")
        .and_then(|value| value.to_str().ok())
        .expect("a cursor for the rest")
        .to_owned();
    let (status, headers, body) = call(
        graph.clone(),
        Method::GET,
        &format!("/rules?limit=2&cursor={cursor}"),
        "application/x-ndjson",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body.lines().filter(|line| !line.is_empty()).count(), 2);
    assert_eq!(
        headers.get("x-has-more").and_then(|v| v.to_str().ok()),
        Some("true")
    );

    // past the last record: no lines and nothing left to ask for
    let cursor = headers
        .get("x-next-cursor")
        .and_then(|value| value.to_str().ok())
        .expect("a cursor for the rest")
        .to_owned();
    let (status, headers, body) = call(
        graph,
        Method::GET,
        &format!("/rules?limit=2&cursor={cursor}"),
        "application/x-ndjson",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body.lines().filter(|line| !line.is_empty()).count(), 1);
    assert_eq!(
        headers.get("x-has-more").and_then(|v| v.to_str().ok()),
        Some("false")
    );
}

#[tokio::test]
async fn asking_for_json_still_gets_exactly_what_it_got_before() {
    let graph = Arc::new(MemoryGraph::new());
    book_with(&graph, 3).await;
    let (status, headers, body) = call(
        graph,
        Method::GET,
        "/rules?limit=2",
        "application/json",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        headers
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok()),
        Some("application/json"),
        "the streaming representation must not leak into the ordinary one"
    );
    let answer: Value = serde_json::from_str(&body).expect("an envelope");
    assert_eq!(answer["count"], json!(2));
    assert_eq!(answer["total"], json!(3));
    assert_eq!(answer["has_more"], json!(true));
}

#[tokio::test]
async fn a_burst_larger_than_the_door_is_served_rather_than_lost() {
    // the door in front of the records router: many more callers than slots, and every one of them must still be answered
    let graph = Arc::new(MemoryGraph::new());
    book_with(&graph, 1).await;
    let governed = vimbai_api::limits::govern(
        vimbai_api::pagination::guard(vimbai_api::records::router(graph)),
        vimbai_api::limits::Limits {
            in_flight: 2,
            timeout: Duration::from_secs(10),
        },
    );
    let mut handles = Vec::new();
    for index in 0..12 {
        let router = governed.clone();
        handles.push(tokio::spawn(async move {
            let request = Request::builder()
                .method(Method::GET)
                .uri(format!("/rules?limit=5&probe={index}"))
                .header("x-user-id", "user-1")
                .header("x-book-id", "book-1")
                .body(Body::empty())
                .unwrap();
            router.oneshot(request).await.unwrap().status()
        }));
    }
    for handle in handles {
        assert_eq!(handle.await.unwrap(), StatusCode::OK);
    }
}

// --- the door ---
//
// These live here rather than beside `limits.rs` on purpose: the contract gate reads every route registered in `src` as an
// address a client can call, and a route that exists only to be slow is not one.

fn a_slow_router() -> axum::Router {
    axum::Router::new().route(
        "/slow",
        axum::routing::get(|| async {
            tokio::time::sleep(Duration::from_millis(80)).await;
            "done"
        }),
    )
}

#[tokio::test]
async fn a_request_that_runs_too_long_is_named_not_dropped() {
    let limited = vimbai_api::limits::govern(
        a_slow_router(),
        vimbai_api::limits::Limits {
            in_flight: 4,
            timeout: Duration::from_millis(20),
        },
    );
    let request = Request::builder().uri("/slow").body(Body::empty()).unwrap();
    let response = limited.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::GATEWAY_TIMEOUT);
    let bytes = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .unwrap();
    let body = String::from_utf8_lossy(&bytes);
    assert!(
        body.contains("20ms"),
        "the answer names the deadline: {body}"
    );
}

#[tokio::test]
async fn more_callers_than_slots_wait_their_turn_rather_than_being_dropped() {
    let limited = vimbai_api::limits::govern(
        a_slow_router(),
        vimbai_api::limits::Limits {
            in_flight: 1,
            timeout: Duration::from_secs(5),
        },
    );
    let started = std::time::Instant::now();
    let mut handles = Vec::new();
    for _ in 0..4 {
        let router = limited.clone();
        handles.push(tokio::spawn(async move {
            router
                .oneshot(Request::builder().uri("/slow").body(Body::empty()).unwrap())
                .await
                .unwrap()
                .status()
        }));
    }
    for handle in handles {
        assert_eq!(handle.await.unwrap(), StatusCode::OK);
    }
    // a single slot means four 80ms requests went through one at a time: the ceiling is real, not decorative
    assert!(
        started.elapsed() >= Duration::from_millis(300),
        "four requests passed a single slot in {:?}",
        started.elapsed()
    );
}
