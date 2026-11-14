#![forbid(unsafe_code)]
//! report-automation-service and report-distribution-service over HTTP, followed through a record's whole life:
//! a template, a schedule on it, a report produced, a list to send it to, and the record of sending.

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use serde_json::{json, Value};
use std::sync::Arc;
use store::fake::MemoryGraph;
use tower::ServiceExt;

async fn call(method: Method, uri: &str, payload: Value) -> (StatusCode, Value) {
    let router = vimbai_api::records::router(Arc::new(MemoryGraph::new()));
    let request = Request::builder()
        .method(method)
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

async fn post(uri: &str, payload: Value) -> (StatusCode, Value) {
    call(Method::POST, uri, payload).await
}

/// One router kept across several calls, so a test can follow a record rather than start afresh each time.
struct Book {
    router: axum::Router,
}

impl Book {
    fn new() -> Self {
        Self {
            router: vimbai_api::records::router(Arc::new(MemoryGraph::new())),
        }
    }

    async fn call(&self, method: Method, uri: &str, payload: Value) -> (StatusCode, Value) {
        let request = Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json")
            .header("x-user-id", "user-1")
            .header("x-book-id", "book-1")
            .body(Body::from(payload.to_string()))
            .unwrap();
        let response = self.router.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 1_000_000)
            .await
            .unwrap();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }
}

#[tokio::test]
async fn a_template_off_the_four_report_types_is_refused_over_http() {
    let (status, body) = post(
        "/templates",
        json!({
            "name": "Odd pack",
            "report_type": "everything",
            "today": "2026-03-01T09:00:00Z"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");

    let (status, body) = post(
        "/templates",
        json!({
            "name": "Board pack",
            "report_type": "financial_summary",
            "sections": ["trial_balance", "cash_flow"],
            "today": "2026-03-01T09:00:00Z"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["sections"][0], json!("trial_balance"));
    assert_eq!(body["format"], json!("pdf"));
}

#[tokio::test]
async fn a_template_a_schedule_a_run_and_the_listing_that_follows() {
    let book = Book::new();
    let (status, template) = book
        .call(
            Method::POST,
            "/templates",
            json!({
                "name": "Board pack",
                "report_type": "financial_summary",
                "sections": ["cash_flow"],
                "today": "2026-03-01T09:00:00Z"
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{template}");
    let template_id = template["id"].as_str().expect("an id").to_string();

    let (status, schedule) = book
        .call(
            Method::POST,
            "/schedules",
            json!({
                "template_id": template_id,
                "name": "Monthly board pack",
                "cron_expression": "0 0 1 * *",
                "recipients": ["board@example.com"],
                "today": "2026-03-01T09:10:00Z"
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{schedule}");
    assert_eq!(schedule["status"], json!("active"));
    let schedule_id = schedule["id"].as_str().expect("an id").to_string();

    let (status, report) = book
        .call(
            Method::POST,
            &format!("/schedules/{schedule_id}/run?today=2026-04-01T06:00:00Z"),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{report}");
    assert_eq!(report["period"], json!("2026-04"));
    assert_eq!(report["data"]["sections"][0], json!("cash_flow"));
    assert_eq!(report["status"], json!("delivered"));

    let (status, listed) = book
        .call(Method::GET, "/generated-reports", json!({}))
        .await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    assert_eq!(listed["reports"].as_array().expect("reports").len(), 1);

    // pausing keeps the report already produced, and stops any further runs
    let (status, paused) = book
        .call(
            Method::PUT,
            &format!("/schedules/{schedule_id}/pause"),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{paused}");
    assert_eq!(paused["status"], json!("paused"));
    let (status, _) = book
        .call(
            Method::POST,
            &format!("/schedules/{schedule_id}/run?today=2026-05-01T06:00:00Z"),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let (_, listed) = book
        .call(Method::GET, "/generated-reports", json!({}))
        .await;
    assert_eq!(listed["reports"].as_array().expect("reports").len(), 1);
}

#[tokio::test]
async fn a_schedule_for_a_template_that_does_not_exist_is_missing() {
    let (status, body) = post(
        "/schedules",
        json!({
            "template_id": "no-such-template",
            "name": "Nowhere",
            "today": "2026-03-01T09:00:00Z"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
}

#[tokio::test]
async fn a_list_a_subscription_on_it_and_the_record_of_sending() {
    let book = Book::new();
    let (status, list) = book
        .call(
            Method::POST,
            "/lists",
            json!({
                "name": "Board",
                "recipients": ["chair@example.com"],
                "today": "2026-03-01T09:00:00Z"
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{list}");
    assert_eq!(list["channels"], json!(["email"]));
    let list_id = list["id"].as_str().expect("an id").to_string();

    let (status, subscription) = book
        .call(
            Method::POST,
            "/subscriptions",
            json!({
                "report_type": "financial_summary",
                "distribution_list_id": list_id,
                "frequency": "monthly",
                "today": "2026-03-01T09:05:00Z"
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{subscription}");
    assert_eq!(subscription["active"], json!(true));
    let sub_id = subscription["id"].as_str().expect("an id").to_string();

    let (status, log) = book
        .call(
            Method::POST,
            &format!("/subscriptions/{sub_id}/distribute?today=2026-04-01T07:00:00Z"),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{log}");
    assert_eq!(log["recipients"][0], json!("chair@example.com"));
    assert_eq!(log["status"], json!("sent"));

    let (status, logs) = book.call(Method::GET, "/logs", json!({})).await;
    assert_eq!(status, StatusCode::OK, "{logs}");
    assert_eq!(logs["logs"].as_array().expect("logs").len(), 1);
}

#[tokio::test]
async fn a_subscription_on_a_frequency_that_is_not_one_of_four_is_refused() {
    let book = Book::new();
    let (_, list) = book
        .call(
            Method::POST,
            "/lists",
            json!({"name": "Ops", "today": "2026-03-01T09:00:00Z"}),
        )
        .await;
    let list_id = list["id"].as_str().expect("an id").to_string();
    let (status, body) = book
        .call(
            Method::POST,
            "/subscriptions",
            json!({
                "report_type": "compliance",
                "distribution_list_id": list_id,
                "frequency": "hourly",
                "today": "2026-03-01T09:05:00Z"
            }),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
}
