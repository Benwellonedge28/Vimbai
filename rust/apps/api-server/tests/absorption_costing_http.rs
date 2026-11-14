//! absorption-costing answers at the three addresses it published for its own arithmetic.
//!
//! The service kept the records (product costs, the latest cost per product, the stock valuation) and calculated three
//! things beside them. Those three now answer at the published addresses from one implementation, exact money
//! throughout, so a client calling what it was told reaches the same arithmetic it always did.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use serde_json::{json, Value};
use store::fake::MemoryGraph;
use tower::ServiceExt;

async fn call(method: Method, uri: &str, payload: Value) -> (StatusCode, Value) {
    let router = vimbai_api::records::router(Arc::new(MemoryGraph::new()));
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json");
    let response = router
        .oneshot(request.body(Body::from(payload.to_string())).unwrap())
        .await
        .unwrap();
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
async fn a_products_cost_per_unit_is_its_production_cost_over_its_units() {
    let (status, answer) = call(
        Method::POST,
        "/absorption-costing/product-costs/calculate",
        json!({
            "product_id": "p-1",
            "product_name": "Widget",
            "period": "2026-11",
            "direct_materials": "1200.00",
            "direct_labor": "800.00",
            "direct_expenses": "300.00",
            "manufacturing_overhead": "700.00",
            "units_produced": "100",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{answer}");
    assert_eq!(answer["prime_cost"], "2300.000000", "{answer}");
    assert_eq!(answer["total_production_cost"], "3000.000000", "{answer}");
    assert_eq!(answer["cost_per_unit"], "30.000000", "{answer}");

    // nothing produced is no unit cost, not a division by zero
    let (status, nothing) = call(
        Method::POST,
        "/product-costs/calculate",
        json!({"direct_materials": "10.00", "units_produced": "0"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{nothing}");
    assert_eq!(nothing["cost_per_unit"], "0.000000", "{nothing}");
}

#[tokio::test]
async fn the_rate_and_what_was_absorbed_cannot_disagree() {
    let (status, answer) = call(
        Method::POST,
        "/absorption-costing/overhead/absorption",
        json!({
            "overhead_cost": "84000.00",
            "absorption_base": "machine_hours",
            "absorption_base_units": "28000",
            "actual_base_units": "28000",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{answer}");
    assert_eq!(answer["overhead_absorption_rate"], "3.000000", "{answer}");
    assert_eq!(answer["absorbed_overhead"], "84000.000000", "{answer}");
    assert_eq!(answer["over_under_absorbed"], "0.000000", "{answer}");

    // a period that worked fewer hours absorbed less than the overhead it was given
    let (_, short) = call(
        Method::POST,
        "/overhead/absorption",
        json!({
            "overhead_cost": "84000.00",
            "absorption_base_units": "28000",
            "actual_base_units": "24000",
        }),
    )
    .await;
    assert_eq!(short["absorbed_overhead"], "72000.000000", "{short}");
    assert_eq!(short["over_under_absorbed"], "-12000.000000", "{short}");
}

#[tokio::test]
async fn a_cost_plus_price_covers_the_cost_and_the_margin_over_it() {
    let (status, answer) = call(
        Method::POST,
        "/absorption-costing/cost-plus",
        json!({"product_cost": "250.00", "markup_percentage": "20"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{answer}");
    assert_eq!(answer["markup_amount"], "50.000000", "{answer}");
    assert_eq!(answer["selling_price"], "300.000000", "{answer}");
}
