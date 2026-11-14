#![forbid(unsafe_code)]
//! The two costing services retired here answer at the prefixes they publish, not at the older engine-side names.
//!
//! basis-apportionment-service and overhead-absorption-rate-service both keep their gateway prefix, so a client asks
//! /basis-apportionment/apportion/floor-area and /overhead-absorption-rate/machine-hours-rate, and those are the
//! addresses these tests walk. The arithmetic itself is the costing crate's and is covered there; what is checked here
//! is that the published address reaches it and comes back with the right figure.

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

fn spread(centres: Value) -> Value {
    json!({
        "cost_type": "rent",
        "total_cost": 1200,
        "cost_centre_data": centres,
    })
}

#[tokio::test]
async fn rent_is_apportioned_by_floor_area_at_the_published_address() {
    let graph = Arc::new(MemoryGraph::new());
    let (status, apportioned) = call(
        Arc::clone(&graph),
        Method::POST,
        "/basis-apportionment/apportion/floor-area",
        Some(spread(json!([
            {"cost_centre_id": "cc-a", "cost_centre_name": "machining", "floor_area": 200},
            {"cost_centre_id": "cc-b", "cost_centre_name": "assembly", "floor_area": 300},
        ]))),
    )
    .await;
    assert!(status.is_success(), "{status}: {apportioned}");
    assert_eq!(apportioned["basis"], "floor_area", "{apportioned}");
    // 200 of 500 square metres takes two fifths of the rent
    assert_eq!(
        apportioned["cost_centres"][0]["apportioned_cost"], "480.000000",
        "{apportioned}"
    );
    assert_eq!(
        apportioned["cost_centres"][1]["apportioned_cost"], "720.000000",
        "{apportioned}"
    );
    // and the parts are the total, not a rounding of it
    assert_eq!(
        apportioned["total_apportioned"], "1200.000000",
        "{apportioned}"
    );
    assert_eq!(apportioned["unapportioned"], "0.000000", "{apportioned}");
}

#[tokio::test]
async fn the_figures_a_centre_holds_are_read_back_under_the_basis_the_service_named() {
    let graph = Arc::new(MemoryGraph::new());
    for (address, key) in [
        ("volume", "volume"),
        ("cost", "asset_cost"),
        ("net-book-value", "net_book_value"),
        ("replacement-value", "replacement_value"),
        ("personnel", "number_of_personnel"),
        ("requisitions", "number_of_requisitions"),
    ] {
        let (status, answer) = call(
            Arc::clone(&graph),
            Method::POST,
            &format!("/basis-apportionment/apportion/{address}"),
            Some(spread(json!([
                {"cost_centre_id": "cc-a", "cost_centre_name": "machining", key: 100},
                {"cost_centre_id": "cc-b", "cost_centre_name": "assembly", key: 100},
            ]))),
        )
        .await;
        assert!(status.is_success(), "{address}: {status}: {answer}");
        assert_eq!(
            answer["total_apportioned"], "1200.000000",
            "{address}: {answer}"
        );
        assert_eq!(
            answer["cost_centres"][0][key],
            json!("100.000000"),
            "{address}"
        );
    }
}

#[tokio::test]
async fn a_standing_rule_is_kept_and_listed_at_the_published_address() {
    let graph = Arc::new(MemoryGraph::new());
    let (status, rule) = call(
        Arc::clone(&graph),
        Method::POST,
        "/basis-apportionment/rules/create",
        Some(json!({
            "cost_type": "rent",
            "basis": "floor_area",
            "description": "Rent follows the space occupied",
        })),
    )
    .await;
    assert!(status.is_success(), "{status}: {rule}");
    assert_eq!(rule["basis"], "floor_area", "{rule}");

    let (status, listed) = call(
        Arc::clone(&graph),
        Method::GET,
        "/basis-apportionment/rules",
        None,
    )
    .await;
    assert!(status.is_success(), "{status}: {listed}");
    assert_eq!(listed["rules"].as_array().unwrap().len(), 1, "{listed}");
}

#[tokio::test]
async fn what_was_apportioned_can_be_read_back() {
    let graph = Arc::new(MemoryGraph::new());
    let (_, _) = call(
        Arc::clone(&graph),
        Method::POST,
        "/basis-apportionment/apportion/floor-area",
        Some(spread(json!([
            {"cost_centre_id": "cc-a", "cost_centre_name": "machining", "floor_area": 200},
        ]))),
    )
    .await;

    let (status, results) = call(
        Arc::clone(&graph),
        Method::GET,
        "/basis-apportionment/results?cost_type=rent",
        None,
    )
    .await;
    assert!(status.is_success(), "{status}: {results}");
    assert!(
        !results["results"].as_array().unwrap().is_empty(),
        "{results}"
    );
}

#[tokio::test]
async fn a_machine_hour_rate_is_answered_at_the_published_address() {
    let graph = Arc::new(MemoryGraph::new());
    let (status, rate) = call(
        Arc::clone(&graph),
        Method::POST,
        "/overhead-absorption-rate/machine-hours-rate",
        Some(json!({
            "cost_centre_name": "machining",
            "budgeted_overhead": 45000,
            "budgeted_machine_hours": 15000,
        })),
    )
    .await;
    assert!(status.is_success(), "{status}: {rate}");
    // 45000 of overhead over 15000 machine hours is 3.00 an hour
    assert_eq!(rate["oar_per_machine_hour"], "3.000000", "{rate}");
    assert_eq!(rate["absorption_base"], "machine_hours", "{rate}");
    assert!(
        rate["analysis_id"].is_string(),
        "the analysis is recorded: {rate}"
    );
}

#[tokio::test]
async fn a_labour_cost_rate_is_a_percentage_of_the_cost_incurred() {
    let graph = Arc::new(MemoryGraph::new());
    let (status, rate) = call(
        Arc::clone(&graph),
        Method::POST,
        "/overhead-absorption-rate/labour-cost-rate",
        Some(json!({
            "cost_centre_name": "assembly",
            "budgeted_overhead": 30000,
            "budgeted_direct_labour_cost": 120000,
        })),
    )
    .await;
    assert!(status.is_success(), "{status}: {rate}");
    // 30000 over 120000 of labour is 25 per cent, and this base answers with the percentage itself
    assert_eq!(rate["oar_percentage"], "25.000000", "{rate}");
}

#[tokio::test]
async fn a_blanket_rate_and_its_absorption_are_answered_at_the_published_address() {
    let graph = Arc::new(MemoryGraph::new());
    let (status, blanket) = call(
        Arc::clone(&graph),
        Method::POST,
        "/overhead-absorption-rate/blanket-rate",
        Some(json!({
            "entity_name": "the factory",
            "absorption_base": "machine_hours",
            "total_budgeted_overhead": 90000,
            "total_base_units": 30000,
        })),
    )
    .await;
    assert!(status.is_success(), "{status}: {blanket}");

    let (status, absorbed) = call(
        Arc::clone(&graph),
        Method::POST,
        "/overhead-absorption-rate/absorbed-overhead",
        Some(json!({
            "cost_centre_name": "machining",
            "absorption_rate": 3,
            "actual_base_units": 28000,
        })),
    )
    .await;
    assert!(status.is_success(), "{status}: {absorbed}");
    // 28000 hours at 3.00 an hour absorbed 84000 of the overhead
    assert_eq!(absorbed["overhead_absorbed"], "84000.000000", "{absorbed}");
}

#[tokio::test]
async fn the_rates_already_struck_can_be_listed() {
    let graph = Arc::new(MemoryGraph::new());
    let (_, _) = call(
        Arc::clone(&graph),
        Method::POST,
        "/overhead-absorption-rate/machine-hours-rate",
        Some(json!({
            "cost_centre_name": "machining",
            "budgeted_overhead": 45000,
            "budgeted_machine_hours": 15000,
        })),
    )
    .await;

    let (status, rates) = call(
        Arc::clone(&graph),
        Method::GET,
        "/overhead-absorption-rate/analyses",
        None,
    )
    .await;
    assert!(status.is_success(), "{status}: {rates}");
    assert!(!rates["analyses"].as_array().unwrap().is_empty(), "{rates}");
}
