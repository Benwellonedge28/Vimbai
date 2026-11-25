#![forbid(unsafe_code)]
//! The fixed-assets register's three ways of writing an asset off, over a real Book.
//!
//! An asset is registered by one of `straight_line`, `reducing_balance` or `time_based`, and the choice is the asset's
//! own: a business writes a van down by reducing balance and a building by straight line in the same register. What
//! these tests pin is that the choice reaches the charge through the route a person uses, that a time-based asset is
//! charged for the days it was held rather than for a whole month, and that a method the register does not know is
//! refused in words instead of quietly taking nothing off the asset.

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

/// An amount compared by value rather than by how many zeros it carries.
fn cents(raw: &Value) -> i128 {
    let text = raw.as_str().unwrap_or("0");
    let (whole, part) = text.split_once('.').unwrap_or((text, ""));
    let negative = whole.starts_with('-');
    let whole: i128 = whole.trim_start_matches('-').parse().unwrap_or(0);
    let pad = format!("{:0<2}", &part[..part.len().min(2)]);
    let total = whole * 100 + pad.parse::<i128>().unwrap_or(0);
    if negative {
        -total
    } else {
        total
    }
}

async fn register(graph: Arc<MemoryGraph>, extra: Value) -> String {
    let mut payload = json!({
        "asset_code": "MV-001",
        "asset_name": "Delivery van",
        "category": "vehicles",
        "acquisition_date": "2026-11-20",
        "acquisition_cost": "240000",
        "useful_life_years": 5,
        "salvage_value": "40000",
    });
    for (key, value) in extra.as_object().expect("an object") {
        payload[key] = value.clone();
    }
    let (status, made) = call(
        graph,
        Method::POST,
        "/records/fixed-assets-register/assets",
        Some(payload),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{made}");
    made["id"].as_str().expect("an id").to_string()
}

#[tokio::test]
async fn a_van_written_down_by_time_is_charged_only_for_the_days_it_was_held() {
    let graph = Arc::new(MemoryGraph::new());
    let id = register(
        graph.clone(),
        json!({"depreciation_method": "time_based", "acquisition_date": "2026-11-20"}),
    )
    .await;
    let (status, entry) = call(
        graph,
        Method::POST,
        &format!("/records/fixed-assets-register/assets/{id}/depreciate?period=2026-11"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{entry}");
    // 40000 a year over 365 days, for the eleven days from the 20th to the end of November
    assert_eq!(cents(&entry["depreciation_amount"]), 120_548);
    assert_eq!(entry["method"], json!("time_based"));
}

#[tokio::test]
async fn a_van_written_down_by_straight_line_is_charged_a_whole_month() {
    let graph = Arc::new(MemoryGraph::new());
    let id = register(
        graph.clone(),
        json!({"depreciation_method": "straight_line", "acquisition_date": "2026-11-20"}),
    )
    .await;
    let (status, entry) = call(
        graph,
        Method::POST,
        &format!("/records/fixed-assets-register/assets/{id}/depreciate?period=2026-11"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{entry}");
    // (240000 - 40000) / 60 months: the day it arrived does not change a straight-line month
    assert_eq!(cents(&entry["depreciation_amount"]), 333_333);
}

#[tokio::test]
async fn a_van_written_down_by_reducing_balance_is_charged_a_share_of_what_is_left() {
    let graph = Arc::new(MemoryGraph::new());
    let id = register(
        graph.clone(),
        json!({"depreciation_method": "reducing_balance", "depreciation_rate": "25"}),
    )
    .await;
    let (status, entry) = call(
        graph,
        Method::POST,
        &format!("/records/fixed-assets-register/assets/{id}/depreciate?period=2026-11"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{entry}");
    // 240000 * 25% / 12, taken off what the asset is carried at
    assert_eq!(cents(&entry["depreciation_amount"]), 500_000);
}

#[tokio::test]
async fn the_three_methods_sit_side_by_side_in_one_register() {
    let graph = Arc::new(MemoryGraph::new());
    let mut expected = Vec::new();
    for method in ["straight_line", "reducing_balance", "time_based"] {
        expected.push((
            method,
            register(graph.clone(), json!({"depreciation_method": method})).await,
        ));
    }
    let (status, listed) = call(
        graph,
        Method::GET,
        "/records/fixed-assets-register/assets",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    let rows = listed.as_array().expect("rows");
    assert_eq!(rows.len(), 3);
    for (method, id) in expected {
        let row = rows
            .iter()
            .find(|row| row["id"] == json!(id))
            .unwrap_or_else(|| panic!("{method} is in the register"));
        assert_eq!(row["depreciation_method"], json!(method));
    }
}

#[tokio::test]
async fn a_method_the_register_does_not_know_is_refused_in_words() {
    let graph = Arc::new(MemoryGraph::new());
    let (status, answer) = call(
        graph,
        Method::POST,
        "/records/fixed-assets-register/assets",
        Some(json!({
            "asset_code": "MV-001",
            "asset_name": "Delivery van",
            "category": "vehicles",
            "acquisition_date": "2026-11-20",
            "acquisition_cost": "240000",
            "useful_life_years": 5,
            "depreciation_method": "units_of_production",
        })),
    )
    .await;
    assert!(!status.is_success(), "{answer}");
    let said = answer.to_string();
    assert!(said.contains("Unknown depreciation method"), "{said}");
    assert!(said.contains("time_based"), "the three are named: {said}");
}

#[tokio::test]
async fn a_month_before_a_time_based_asset_arrived_is_refused() {
    let graph = Arc::new(MemoryGraph::new());
    let id = register(
        graph.clone(),
        json!({"depreciation_method": "time_based", "acquisition_date": "2026-11-20"}),
    )
    .await;
    let (status, answer) = call(
        graph,
        Method::POST,
        &format!("/records/fixed-assets-register/assets/{id}/depreciate?period=2026-10"),
        None,
    )
    .await;
    assert!(!status.is_success(), "{answer}");
    assert!(
        answer.to_string().contains("before the asset was acquired"),
        "{answer}"
    );
}
