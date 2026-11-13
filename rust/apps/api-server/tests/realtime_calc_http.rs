//! realtime-calculation-engine over HTTP: four figures, each answered by the calculator that already owns it.

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use serde_json::{json, Value};
use std::sync::Arc;
use store::fake::MemoryGraph;
use tower::ServiceExt;

fn router() -> axum::Router {
    vimbai_api::records::router(Arc::new(MemoryGraph::new()))
}

async fn call(router: &axum::Router, uri: &str, payload: Value) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(Method::POST)
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
async fn the_four_figures_are_answered_by_the_calculators_that_already_own_them() {
    let router = router();

    // the net worth today of a series, at a rate stated as a percent
    let (status, npv) = call(
        &router,
        "/npv",
        json!({"initial_investment": 1000, "cash_flows": [500, 500, 500], "discount_rate": 10}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{npv}");
    assert_eq!(npv["npv"], json!(243.43));
    assert_eq!(npv["profitable"], json!(true));

    // the rate at which the same series is worth exactly what it cost
    let (status, irr) = call(
        &router,
        "/irr",
        json!({"initial_investment": 1000, "cash_flows": [1100]}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{irr}");
    assert_eq!(irr["irr"], json!(10.0));

    // a schedule by one of the three named methods
    let (status, depreciation) = call(
        &router,
        "/depreciation",
        json!({"cost": 1000, "salvage_value": 100, "useful_life": 3, "method": "straight_line"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{depreciation}");
    assert_eq!(depreciation["total_depreciable"], json!(900.0));
    assert_eq!(depreciation["schedule"][2]["book_value"], json!(100.0));

    // a loan paid down month by month, which lands on nothing
    let (status, loan) = call(
        &router,
        "/amortize",
        json!({"principal": 1000, "annual_rate": 12, "years": 1}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{loan}");
    assert_eq!(loan["monthly_payment"], json!(88.85));
    assert_eq!(loan["schedule"][11]["balance"], json!(0.0));

    // the loan door takes its three figures from the query string too, as the shipped one did
    let request = Request::builder()
        .method(Method::POST)
        .uri("/amortize?principal=1200&annual_rate=0&years=1")
        .header("x-user-id", "user-1")
        .header("x-book-id", "book-1")
        .body(Body::empty())
        .unwrap();
    let response = router.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), 1_000_000)
        .await
        .unwrap();
    let queried: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(queried["monthly_payment"], json!(100.0));

    // a series of nothing is refused rather than answered with a guess
    let (status, _) = call(
        &router,
        "/npv",
        json!({"initial_investment": 1000, "cash_flows": [], "discount_rate": 10}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let (status, _) = call(
        &router,
        "/irr",
        json!({"initial_investment": 1000, "cash_flows": []}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}
