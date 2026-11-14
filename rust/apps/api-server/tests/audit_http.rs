#![forbid(unsafe_code)]
//! audit-engagement-service and audit-sampling-service over HTTP: an engagement opened, moved on, given a finding,
//! remediated and reported; and a sample drawn from a population. These tests are about the wiring - that each
//! path reaches its own handler with the Book it was asked in - so they check the answers a caller would see
//! rather than re-testing arithmetic the crate's own tests cover.

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use serde_json::{json, Value};
use std::sync::Arc;
use store::fake::MemoryGraph;
use tower::ServiceExt;

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
        self.answer(request).await
    }

    async fn get(&self, uri: &str) -> (StatusCode, Value) {
        let request = Request::builder()
            .method(Method::GET)
            .uri(uri)
            .header("x-user-id", "user-1")
            .header("x-book-id", "book-1")
            .body(Body::empty())
            .unwrap();
        self.answer(request).await
    }

    async fn answer(&self, request: Request<Body>) -> (StatusCode, Value) {
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

async fn an_engagement(book: &Book) -> String {
    let (status, created) = book
        .call(
            Method::POST,
            "/engagements",
            json!({
                "company_id": "co-1",
                "audit_type": "tax",
                "title": "FY26 tax audit",
                "objectives": ["Completeness"],
                "auditor": "Z. Moyo",
                "today": "2026-02-01T09:00:00Z"
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    created["id"].as_str().expect("an id").to_string()
}

#[tokio::test]
async fn an_engagement_is_opened_and_listed() {
    let book = Book::new();
    let id = an_engagement(&book).await;
    assert!(!id.is_empty());

    let (status, listing) = book.get("/engagements/co-1").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(listing["total"], json!(1));
    assert_eq!(listing["engagements"][0]["status"], json!("planned"));
    assert_eq!(listing["engagements"][0]["audit_type"], json!("tax"));

    // another Book sees nothing of it
    let request = Request::builder()
        .method(Method::GET)
        .uri("/engagements/co-1")
        .header("x-user-id", "user-2")
        .header("x-book-id", "book-2")
        .body(Body::empty())
        .unwrap();
    let (_, theirs) = book.answer(request).await;
    assert_eq!(theirs["total"], json!(0));
}

#[tokio::test]
async fn the_audit_families_are_answered() {
    let book = Book::new();
    let (status, families) = book.get("/audit-types").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        families["audit_types"],
        json!(["tax", "forensic", "it", "operational"])
    );
}

#[tokio::test]
async fn an_engagement_moves_on_and_a_finding_is_raised_remediated_and_reported() {
    let book = Book::new();
    let id = an_engagement(&book).await;

    let (status, moved) = book
        .call(
            Method::PUT,
            &format!("/engagements/{id}/status"),
            json!({"status": "in_progress"}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{moved}");
    assert_eq!(moved["status"], json!("in_progress"));

    let (status, raised) = book
        .call(
            Method::POST,
            &format!("/engagements/{id}/findings"),
            json!({
                "title": "Missing invoice",
                "description": "Two purchases have no supporting invoice",
                "severity": "high"
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{raised}");
    assert_eq!(raised["severity"], json!("high"));
    let finding_id = raised["finding_id"]
        .as_str()
        .expect("a finding id")
        .to_string();

    let (status, reported) = book.get(&format!("/engagement-report/{id}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(reported["findings_summary"]["high"], json!(1));
    assert_eq!(reported["open_findings"], json!(1));

    let (status, remediated) = book
        .call(
            Method::PUT,
            &format!("/findings/{finding_id}/remediate"),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{remediated}");
    assert_eq!(remediated["status"], json!("remediated"));

    let (_, after) = book.get(&format!("/engagement-report/{id}")).await;
    assert_eq!(after["remediated"], json!(1));
    assert_eq!(after["open_findings"], json!(0));
}

#[tokio::test]
async fn a_finding_on_an_engagement_that_is_not_mine_is_missing() {
    let book = Book::new();
    let id = an_engagement(&book).await;
    let request = Request::builder()
        .method(Method::POST)
        .uri(format!("/engagements/{id}/findings"))
        .header("content-type", "application/json")
        .header("x-user-id", "user-2")
        .header("x-book-id", "book-2")
        .body(Body::from(
            json!({"title": "Theirs", "description": "x"}).to_string(),
        ))
        .unwrap();
    let (status, _) = book.answer(request).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_sample_is_drawn_over_http() {
    let book = Book::new();
    let population: Vec<Value> = (0..40)
        .map(|index| {
            json!({
                "item_id": format!("T-{index:03}"),
                "date": "2026-01-15",
                "amount": 100.0 + index as f64,
                "description": format!("Sale {index}"),
                "risk_score": if index % 2 == 0 { 80 } else { 20 }
            })
        })
        .collect();
    let (status, drawn) = book
        .call(
            Method::POST,
            "/sample",
            json!({
                "audit_id": "A-1",
                "account_id": "ACC-1",
                "population": population,
                "parameters": {
                    "population_size": 40,
                    "expected_error_rate": 0.05,
                    "tolerable_error_rate": 0.05,
                    "confidence_level": 95.0,
                    "sampling_method": "attribute"
                },
                "stratification": false,
                "random_seed": 42
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{drawn}");
    assert_eq!(drawn["sample_size"], json!(40));
    assert_eq!(drawn["margin_of_error"], json!(0.025));
    assert_eq!(drawn["stratification_levels"]["high"], json!(20));
    assert_eq!(
        drawn["statistical_conclusion"],
        json!("Sample of 40 items provides 9500% confidence")
    );
}

#[tokio::test]
async fn a_sample_with_no_tolerance_is_refused() {
    let book = Book::new();
    let (status, answer) = book
        .call(
            Method::POST,
            "/sample",
            json!({
                "audit_id": "A-1",
                "account_id": "ACC-1",
                "population": [{"item_id": "T-1", "amount": 10.0}],
                "parameters": {
                    "population_size": 1,
                    "expected_error_rate": 0.05,
                    "tolerable_error_rate": 0.0,
                    "confidence_level": 95.0,
                    "sampling_method": "attribute"
                },
                "stratification": false
            }),
        )
        .await;
    // a refused argument comes back as unprocessable, which is what the server answers for Invalid
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{answer}");
}
