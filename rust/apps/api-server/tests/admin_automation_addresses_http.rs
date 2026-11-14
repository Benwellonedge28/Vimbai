#![forbid(unsafe_code)]
//! admin and the automation engine answer at the prefixes they publish.
//!
//! Both had their rules built already: the admin crate files features under eleven categories, and the automation crate
//! keeps every run a rule has been through. What the services published was the address - the categories a feature is
//! filed under, and the runs behind a rule - so these walk those addresses and nothing else.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use serde_json::{json, Value};
use store::fake::MemoryGraph;
use tower::ServiceExt;

async fn call(
    graph: Arc<MemoryGraph>,
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
    let bytes = axum::body::to_bytes(response.into_body(), 2_000_000)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[tokio::test]
async fn the_categories_a_feature_is_filed_under_answer_under_admin() {
    let graph = Arc::new(MemoryGraph::new());
    let (status, categories) = call(
        Arc::clone(&graph),
        Method::GET,
        "/admin/features/categories",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{categories}");
    let listed = categories.as_array().expect("a list of categories");
    assert!(listed.len() >= 11, "{categories}");
    assert!(
        listed.iter().any(|entry| entry["value"] == "accounting"),
        "{categories}"
    );
}

#[tokio::test]
async fn a_rules_runs_answer_under_the_automation_prefix() {
    let graph = Arc::new(MemoryGraph::new());
    let (status, runs) = call(
        Arc::clone(&graph),
        Method::GET,
        "/automation/executions",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{runs}");

    let (status, rules) = call(Arc::clone(&graph), Method::GET, "/automation/rules", None).await;
    assert_eq!(status, StatusCode::OK, "{rules}");

    // the engine still answers its own names, so nothing that already called them has to move
    let (status, engine_side) = call(graph, Method::GET, "/automation-executions", None).await;
    assert_eq!(status, StatusCode::OK, "{engine_side}");
    let _ = json!(null);
}
