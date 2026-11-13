//! reporting-service and dashboard-service over HTTP: a report asked for, a schedule recorded, and a layout read.

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use serde_json::{json, Value};
use std::sync::Arc;
use store::{Graph, StoreError};
use tower::ServiceExt;

struct Empty;

impl Graph for Empty {
    fn run(&self, _cypher: &str, _params: Value) -> Result<Vec<Value>, StoreError> {
        Ok(Vec::new())
    }
}

async fn post(uri: &str, payload: Value) -> (StatusCode, Value) {
    let router = vimbai_api::records::router(Arc::new(Empty));
    let request = Request::builder()
        .method(Method::POST)
        .uri(uri)
        .header("content-type", "application/json")
        .header("x-user-id", "user-1")
        .header("x-book-id", "book-1")
        .body(Body::from(payload.to_string()))
        .unwrap();
    let response = router.oneshot(request).await.unwrap();
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
async fn a_report_is_asked_for_and_answers_with_the_ask() {
    let (status, body) = post(
        "/generate",
        json!({
            "company_id": "co-1",
            "report_type": "trial_balance",
            "period_start": "2026-03-01",
            "period_end": "2026-03-31",
            "format": "pdf",
            "recipients": ["finance@example.com"],
            "today": "2026-03-31T14:05:00+00:00"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["report_id"], json!("RPT-202603311405"));
    assert_eq!(body["status"], json!("generated"));
    assert_eq!(body["placeholder"], json!(true));
}

#[tokio::test]
async fn a_report_over_no_period_is_refused() {
    let (status, body) = post(
        "/generate",
        json!({"company_id": "co-1", "report_type": "cash_flow", "today": "2026-03-31T14:05:00"}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
}

#[tokio::test]
async fn a_schedule_is_recorded_and_says_nothing_will_run_it() {
    let (status, body) = post(
        "/schedule",
        json!({
            "company_id": "co-1",
            "report_type": "cash_flow",
            "schedule": "monthly",
            "recipients": [],
            "parameters": {},
            "today": "2026-03-31T14:05:00+00:00"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["schedule_id"], json!("SCH-202603311405"));
    assert_eq!(body["active"], json!(true));
    assert_eq!(body["placeholder"], json!(true));
}

#[tokio::test]
async fn the_operations_layout_is_loaded_for_a_period() {
    let (status, body) = post(
        "/load",
        json!({
            "company_id": "co-1",
            "dashboard_type": "operations",
            "period_start": "2026-03-01",
            "period_end": "2026-03-31",
            "today": "2026-03-31T14:05:00+00:00"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["dashboard_id"], json!("DASH-co-1-operations-20260331"));
    assert_eq!(body["widgets"].as_array().expect("widgets").len(), 4);
    assert_eq!(body["period_end"], json!("2026-03-31"));
}

#[tokio::test]
async fn a_custom_dashboard_keeps_the_widgets_it_was_given() {
    let (status, body) = post(
        "/custom",
        json!({
            "company_id": "co-1",
            "dashboard_type": "board",
            "dashboard_key": "abc12345",
            "today": "2026-03-31T14:05:00+00:00",
            "widgets": [
                {"type": "kpi_card", "title": "Revenue", "data": {"value": 1200}, "position": 7},
                {"type": "table", "title": "Debtors"}
            ]
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["dashboard_id"], json!("CUST-co-1-abc12345"));
    assert_eq!(body["widgets"][0]["position"], json!(7));
    assert_eq!(body["widgets"][1]["widget_id"], json!("c2"));
}
