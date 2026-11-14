#![forbid(unsafe_code)]
//! treasury-analytics-service answers at the addresses it published.
//!
//! The analysis itself always lived in the treasury crate; what these tests pin is that a caller who was told
//! /treasury-analytics keeps a working door - the health check a monitor watched, the analysis, and the stored snapshot
//! read back by company - and that the figures are the ones the Python service answered.

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

fn analysis() -> Value {
    json!({
        "company_id": "comp-1",
        "total_cash": 500000,
        "monthly_inflow": 200000,
        "monthly_outflow": 150000,
        "short_term_debt": 30000,
        "total_debt": 100000,
        "investments": 200000,
        "fx_exposure": 50000,
    })
}

#[tokio::test]
async fn the_health_door_the_monitor_watched_still_answers() {
    let (status, body) = call(
        Arc::new(MemoryGraph::default()),
        Method::GET,
        "/treasury-analytics/health",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], json!("healthy"));
}

#[tokio::test]
async fn the_published_analysis_answers_with_the_python_services_figures() {
    let (status, body) = call(
        Arc::new(MemoryGraph::default()),
        Method::POST,
        "/treasury-analytics/analyze",
        Some(analysis()),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    // 500,000 of cash against 150,000 of monthly burn is 100 days; 30,000 of short-term debt against
    // 200,000 of inflow is 15 per cent. Those are the numbers the Python service answered.
    assert_eq!(body["cash_adequacy_days"], json!("100.000000"));
    assert_eq!(body["debt_service_ratio"], json!("15.000000"));
    assert!(body["kpis"].as_array().expect("kpis").len() >= 5, "{body}");
}

#[tokio::test]
async fn the_snapshot_is_read_back_by_company() {
    let graph = Arc::new(MemoryGraph::default());
    let (status, _) = call(
        graph.clone(),
        Method::POST,
        "/treasury-analytics/analyze",
        Some(analysis()),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, body) = call(graph, Method::GET, "/treasury-analytics/kpi/comp-1", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["cash_adequacy_days"], json!("100.000000"));

    // a company nobody analysed gets the nudge, not a made-up zero
    let (status, body) = call(
        Arc::new(MemoryGraph::default()),
        Method::GET,
        "/treasury-analytics/kpi/nobody",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["message"], json!("Run /analyze first"));
}
