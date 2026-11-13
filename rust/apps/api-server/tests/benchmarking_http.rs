//! performance-benchmarking-service over HTTP: a metric, a placement, and the peers held for it.

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use serde_json::{json, Value};
use std::sync::Arc;
use store::fake::MemoryGraph;
use tower::ServiceExt;

fn router() -> axum::Router {
    vimbai_api::records::router(Arc::new(MemoryGraph::new()))
}

async fn call(
    router: &axum::Router,
    method: Method,
    uri: &str,
    payload: Value,
) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .header("x-user-id", "user-1")
        .header("x-book-id", "book-1")
        .body(Body::from(payload.to_string()))
        .unwrap();
    let response = router.clone().oneshot(request).await.unwrap();
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
async fn a_metric_places_a_figure_and_keeps_its_peers() {
    let router = router();

    let (status, defined) = call(
        &router,
        Method::POST,
        "/metrics",
        json!({"name": "return on assets", "category": "financial", "unit": "percentage",
               "industry_median": 10.0, "industry_top_quartile": 20.0,
               "industry_bottom_quartile": 5.0}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{defined}");
    let metric_id = defined["id"].as_str().expect("an id").to_string();

    // a fifth category is a bad request
    let (status, _) = call(
        &router,
        Method::POST,
        "/metrics",
        json!({"name": "mood", "category": "sentiment"}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // a figure is placed in its band, and the listing narrows by category
    let (status, placed) = call(
        &router,
        Method::POST,
        "/benchmark",
        json!({"metric_id": metric_id, "organization_value": 15.0, "period": "2026-Q1"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{placed}");
    assert_eq!(placed["percentile_rank"], json!(62.5));
    assert_eq!(placed["rating"], json!("above_average"));
    assert_eq!(placed["period"], json!("2026-Q1"));
    let (_, listed) = call(
        &router,
        Method::GET,
        "/metrics?category=financial",
        json!({}),
    )
    .await;
    assert_eq!(listed.as_array().map(Vec::len), Some(1));
    let (_, other) = call(&router, Method::GET, "/metrics?category=growth", json!({})).await;
    assert_eq!(other.as_array().map(Vec::len), Some(0));

    // a metric nobody defined cannot place anything
    let (status, _) = call(
        &router,
        Method::POST,
        "/benchmark",
        json!({"metric_id": "nope", "organization_value": 1.0, "period": "2026-Q1"}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // leaders run past a hundred rather than being capped
    let (_, ahead) = call(
        &router,
        Method::POST,
        "/benchmark",
        json!({"metric_id": metric_id, "organization_value": 60.0, "period": "2026-Q2"}),
    )
    .await;
    assert!(
        ahead["percentile_rank"].as_f64().unwrap_or(0.0) > 100.0,
        "{ahead}"
    );
    assert_eq!(ahead["rating"], json!("top_quartile"));

    // the results listing keeps the trailing slice
    let (_, results) = call(&router, Method::GET, "/results?limit=1", json!({})).await;
    let results = results.as_array().expect("results");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["organization_value"], json!(60.0));

    // a peer figure is accepted before its metric is written down here, and read back by metric and period
    let (status, peer) = call(
        &router,
        Method::POST,
        "/peer-comparisons",
        json!({"metric_id": "roa", "organization_name": "Moyo Traders", "value": 12.5, "period": "2026-Q1"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{peer}");
    assert_eq!(peer["organization_name"], json!("Moyo Traders"));
    let (_, peers) = call(&router, Method::GET, "/peer-comparisons/roa", json!({})).await;
    assert_eq!(peers.as_array().map(Vec::len), Some(1));
    let (_, quarter) = call(
        &router,
        Method::GET,
        "/peer-comparisons/roa?period=2026-Q2",
        json!({}),
    )
    .await;
    assert_eq!(quarter.as_array().map(Vec::len), Some(0));
}
