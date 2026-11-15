#![forbid(unsafe_code)]
//! The two addresses the app used to call that nothing served, answered where they now point.
//!
//! The app posted a balance sheet it had worked out itself to `/ratio-analysis/ratios`, and a household's incomes, expenses,
//! assets and liabilities to `/household-finance/analyze`. Neither prefix was ever routed; both calculations existed in Rust and
//! only `/calc` was a door to them. These ask the app's own payloads at the addresses the app now uses, so a change to either
//! calculator's shape shows up here rather than as a 404 on a screen.

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use serde_json::{json, Value};
use std::sync::Arc;
use store::fake::MemoryGraph;
use tower::ServiceExt;

fn router() -> axum::Router {
    // the assembled engine, because `/calc` is the calculator door and the record routes are mounted beside it
    let graph: vimbai_api::records::GraphHandle = Arc::new(MemoryGraph::default());
    vimbai_api::routed(Some(graph))
}

async fn post(uri: &str, payload: Value) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(Method::POST)
        .uri(uri)
        .header("content-type", "application/json")
        .header("x-user-id", "user-1")
        .header("x-book-id", "book-1")
        .body(Body::from(payload.to_string()))
        .unwrap();
    let response = router().oneshot(request).await.unwrap();
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
async fn the_ratios_the_budget_screen_works_out_are_answered_at_the_calculator_door() {
    // the payload `FinanceApiService.getFinancialRatios` builds
    let (status, body) = post(
        "/calc/ratio-analysis-ratios",
        json!({
            "company_id": "book-1",
            "period": "2026-11",
            "balance_sheet": {
                "current_assets": 240000.0,
                "current_liabilities": 120000.0,
                "total_assets": 600000.0,
                "total_debt": 200000.0,
                "total_equity": 400000.0,
                "inventory": 60000.0,
                "cash": 90000.0,
                "marketable_securities": 30000.0,
                "accounts_receivable": 50000.0,
                "accounts_payable": 70000.0
            },
            "income_statement": {
                "revenue": 900000.0,
                "net_income": 90000.0,
                "cogs": 540000.0,
                "interest_expense": 10000.0
            },
            "cash_flow": {}
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    // a current ratio of 2 from 240000 over 120000, and a quick ratio of 1.5 once the 60000 of stock is taken out
    let liquidity = &body["liquidity_ratios"];
    assert_eq!(liquidity["current_ratio"], 2.0, "{body}");
    assert_eq!(liquidity["quick_ratio"], 1.5, "{body}");
    // the shape the screen reads: a section per reading rather than a flat list of numbers
    assert_eq!(body["profitability_ratios"]["net_margin"], 10.0, "{body}");
    assert_eq!(body["leverage_ratios"]["debt_to_equity"], 0.5, "{body}");
}

#[tokio::test]
async fn the_household_the_app_analyses_is_answered_at_the_calculator_door() {
    let (status, body) = post(
        "/calc/household-finance-analyze",
        json!({
            "household_id": "H1",
            "period": "2026-11",
            "incomes": [{"name": "salary", "amount": 5000.0, "frequency": "monthly"}],
            "expenses": [{"name": "rent", "amount": 1500.0, "frequency": "monthly"}],
            "assets": [{"name": "savings", "amount": 20000.0, "is_liquid": true}],
            "liabilities": [{"name": "loan", "amount": 8000.0, "interest_rate": 0.1}]
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["household_id"], "H1");
    assert_eq!(body["period"], "2026-11");
}
