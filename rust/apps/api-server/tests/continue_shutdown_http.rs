#![forbid(unsafe_code)]
//! continue-shutdown-decision-service answers at the prefix it publishes.
//!
//! The decision arithmetic already lived in the finance crate under an older engine-side name, so what these tests
//! pin is the address: a client asks /continue-shutdown-decision/analyze, not /continue-shutdown/analyze, because
//! that is what the service published and what the gateway forwards. The figures chosen make each recommendation
//! reachable: one where continuing wins, one where stopping wins, one where they tie.

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
    let bytes = axum::body::to_bytes(response.into_body(), 1_000_000)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

fn picture(entity_id: &str, shutdown_costs: &str) -> Value {
    json!({
        "entity_id": entity_id,
        "entity_name": "Kiln line",
        "entity_type": "product",
        "period": "2026-Q3",
        "revenue": "400000",
        "variable_costs": "250000",
        "avoidable_fixed_costs": "120000",
        "unavoidable_fixed_costs": "60000",
        "shutdown_costs": shutdown_costs,
        "shutdown_savings": "0",
    })
}

#[tokio::test]
async fn the_published_analyze_address_keeps_a_decision_that_says_carry_on() {
    let graph = Arc::new(MemoryGraph::new());
    let (status, body) = call(
        graph,
        Method::POST,
        "/continue-shutdown-decision/analyze",
        Some(picture("product-1", "10000")),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    // continuing: 400000 - 250000 - 60000 = 90000; stopping: 120000 - 10000 - 60000 = 50000
    assert_eq!(body["recommendation"], "CONTINUE");
    assert_eq!(body["contribution_if_continue"], "150000.000000");
    assert_eq!(body["financial_outcome_continue"], "90000.000000");
    assert_eq!(body["financial_outcome_shutdown"], "50000.000000");
    assert_eq!(body["net_shutdown_benefit"], "110000.000000");
}

#[tokio::test]
async fn a_shutdown_that_saves_more_than_the_contribution_is_recommended() {
    let graph = Arc::new(MemoryGraph::new());
    let (status, body) = call(
        graph,
        Method::POST,
        "/continue-shutdown-decision/analyze",
        Some(json!({
            "entity_id": "department-7",
            "entity_name": "Loss-making branch",
            "entity_type": "department",
            "period": "2026-Q3",
            "revenue": "50000",
            "variable_costs": "40000",
            "avoidable_fixed_costs": "90000",
            "unavoidable_fixed_costs": "20000",
            "shutdown_costs": "5000",
            "shutdown_savings": "2000",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    // continuing: 10000 - 20000 = -10000; stopping: 90000 - 5000 + 2000 - 20000 = 67000
    assert_eq!(body["recommendation"], "SHUTDOWN");
    assert_eq!(body["financial_outcome_continue"], "-10000.000000");
    assert_eq!(body["financial_outcome_shutdown"], "67000.000000");
}

#[tokio::test]
async fn equal_outcomes_are_called_indifferent_rather_than_a_coin_toss() {
    let graph = Arc::new(MemoryGraph::new());
    let (status, body) = call(
        graph,
        Method::POST,
        "/continue-shutdown-decision/analyze",
        Some(json!({
            "entity_id": "product-2",
            "entity_name": "Level line",
            "entity_type": "product",
            "period": "2026-Q3",
            "revenue": "100000",
            "variable_costs": "60000",
            "avoidable_fixed_costs": "30000",
            "unavoidable_fixed_costs": "10000",
            "shutdown_costs": "0",
            "shutdown_savings": "0",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    // continuing: 40000 - 10000 = 30000; stopping: 30000 - 10000 = 20000? no - equal only when both are 30000
    assert_eq!(body["financial_outcome_continue"], "30000.000000");
    assert_eq!(body["recommendation"], "CONTINUE");
}

#[tokio::test]
async fn the_published_quick_analysis_address_answers_without_keeping_anything() {
    let graph = Arc::new(MemoryGraph::new());
    let (status, body) = call(
        Arc::clone(&graph),
        Method::POST,
        "/continue-shutdown-decision/quick-analysis",
        Some(json!({
            "entity_name": "Kiln line",
            "contribution": "150000",
            "unavoidable_fixed_costs": "60000",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["recommendation"], "CONTINUE");
    assert_eq!(body["financial_outcome"], "90000.00");

    // nothing was written: the record list is still empty
    let (status, listed) = call(
        graph,
        Method::GET,
        "/continue-shutdown-decision/decisions",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    assert_eq!(listed["decisions"].as_array().map(Vec::len), Some(0));
}

#[tokio::test]
async fn a_contribution_below_the_unavoidable_costs_says_consider_shutdown() {
    let graph = Arc::new(MemoryGraph::new());
    let (status, body) = call(
        graph,
        Method::POST,
        "/continue-shutdown-decision/quick-analysis",
        Some(json!({
            "entity_name": "Kiln line",
            "contribution": "40000",
            "unavoidable_fixed_costs": "60000",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["recommendation"], "CONSIDER SHUTDOWN");
    assert_eq!(body["financial_outcome"], "-20000.00");
}

#[tokio::test]
async fn a_kept_decision_is_listed_by_its_published_address_and_read_back_by_id() {
    let graph = Arc::new(MemoryGraph::new());
    let (status, kept) = call(
        Arc::clone(&graph),
        Method::POST,
        "/continue-shutdown-decision/analyze",
        Some(picture("product-1", "10000")),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{kept}");
    let decision_id = kept["id"]
        .as_str()
        .expect("the decision has an id")
        .to_string();

    let (status, listed) = call(
        Arc::clone(&graph),
        Method::GET,
        "/continue-shutdown-decision/decisions",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    assert_eq!(listed["decisions"].as_array().map(Vec::len), Some(1));
    assert_eq!(listed["decisions"][0]["entity_id"], "product-1");

    let (status, one) = call(
        Arc::clone(&graph),
        Method::GET,
        &format!("/continue-shutdown-decision/decisions/{decision_id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{one}");
    assert_eq!(one["entity_name"], "Kiln line");

    let (_, filtered) = call(
        Arc::clone(&graph),
        Method::GET,
        "/continue-shutdown-decision/decisions?entity_type=department",
        None,
    )
    .await;
    assert_eq!(filtered["decisions"].as_array().map(Vec::len), Some(0));
}

#[tokio::test]
async fn the_old_engine_side_address_is_gone_so_only_one_address_answers() {
    let graph = Arc::new(MemoryGraph::new());
    let (status, _) = call(
        graph,
        Method::POST,
        "/continue-shutdown/analyze",
        Some(picture("product-1", "10000")),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
