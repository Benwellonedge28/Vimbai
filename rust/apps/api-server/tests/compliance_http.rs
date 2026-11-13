//! sox-compliance-service and risk-assessment-service over HTTP, followed through a record's whole life: a control,
//! a test against it, the deficiency that test raises, and a risk scored and reassessed.

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

async fn post(uri: &str, payload: Value) -> (StatusCode, Value) {
    Book::new().call(Method::POST, uri, payload).await
}

#[tokio::test]
async fn a_control_a_test_and_the_deficiency_it_raises() {
    let book = Book::new();
    let (status, control) = book
        .call(
            Method::POST,
            "/controls",
            json!({
                "control_id_ref": "SOX-ITGC-001",
                "description": "Access reviews each quarter",
                "control_type": "detective",
                "control_nature": "manual",
                "frequency": "quarterly",
                "owner": "Head of IT",
                "process": "IT",
                "today": "2026-03-01T09:00:00Z"
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{control}");
    assert_eq!(control["status"], json!("active"));
    assert_eq!(control["risk_level"], json!("medium"));
    let control_id = control["id"].as_str().expect("an id").to_string();

    let (status, tested) = book
        .call(
            Method::POST,
            &format!("/controls/{control_id}/test"),
            json!({
                "test_period": "2026-Q1",
                "tester": "Internal audit",
                "sample_size": 25,
                "exceptions_found": 6,
                "today": "2026-04-01T09:00:00Z"
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{tested}");
    assert_eq!(tested["result"], json!("fail"));

    let (status, raised) = book.call(Method::GET, "/deficiencies", json!({})).await;
    assert_eq!(status, StatusCode::OK, "{raised}");
    let rows = raised.as_array().expect("rows");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["severity"], json!("significant_deficiency"));

    let (status, board) = book.call(Method::GET, "/sox-dashboard", json!({})).await;
    assert_eq!(status, StatusCode::OK, "{board}");
    assert_eq!(board["total_controls"], json!(1));
    assert_eq!(board["failing_tests"], json!(1));
    assert_eq!(board["open_deficiencies"], json!(1));

    let deficiency_id = rows[0]["id"].as_str().expect("an id").to_string();
    let (status, moved) = book
        .call(
            Method::PUT,
            &format!("/deficiencies/{deficiency_id}"),
            json!({"status": "remediated", "today": "2026-05-01T10:00:00Z"}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{moved}");
    assert_eq!(moved["remediated_date"], json!("2026-05-01T10:00:00Z"));
}

#[tokio::test]
async fn a_test_against_a_control_that_does_not_exist_is_missing() {
    let (status, body) = post(
        "/controls/no-such-control/test",
        json!({"test_period": "2026-Q1", "tester": "me"}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
}

#[tokio::test]
async fn a_deficiency_of_an_unknown_severity_is_refused_and_the_three_real_ones_are_taken() {
    let (status, body) = post(
        "/deficiencies",
        json!({
            "control_id": "C-1",
            "severity": "minor",
            "description": "Small",
            "today": "2026-04-01T09:00:00Z"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");

    let (status, body) = post(
        "/deficiencies",
        json!({
            "control_id": "C-1",
            "severity": "material_weakness",
            "description": "No segregation of duties",
            "today": "2026-04-01T09:00:00Z"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["status"], json!("open"));
}

#[tokio::test]
async fn a_risk_is_scored_on_the_way_in_and_again_when_it_is_reassessed() {
    let book = Book::new();
    let (status, created) = book
        .call(
            Method::POST,
            "/risks",
            json!({
                "company_id": "co-1",
                "category": "financial",
                "name": "Customer concentration",
                "likelihood": 5,
                "impact": 5,
                "today": "2026-03-01T09:00:00Z"
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    assert_eq!(created["risk_score"], json!(25));
    assert_eq!(created["level"], json!("critical"));
    let risk_id = created["id"].as_str().expect("an id").to_string();

    let (status, listed) = book.call(Method::GET, "/risks/co-1", json!({})).await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    assert_eq!(listed["total"], json!(1));
    assert_eq!(listed["by_level"]["critical"], json!(1));
    assert_eq!(listed["by_level"]["low"], json!(0));

    let (status, moved) = book
        .call(
            Method::PUT,
            &format!("/risks/{risk_id}"),
            json!({"likelihood": 2, "impact": 2, "today": "2026-04-01T09:00:00Z"}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{moved}");
    assert_eq!(moved["level"], json!("low"));

    let (status, board) = book
        .call(Method::GET, "/risk-dashboard/co-1", json!({}))
        .await;
    assert_eq!(status, StatusCode::OK, "{board}");
    assert_eq!(board["total_risks"], json!(1));
    assert_eq!(board["avg_score"], json!(4.0));

    let (status, closed) = book
        .call(Method::DELETE, &format!("/risks/{risk_id}"), json!({}))
        .await;
    assert_eq!(status, StatusCode::OK, "{closed}");
    assert_eq!(closed["status"], json!("closed"));
}

#[tokio::test]
async fn a_company_with_no_risks_answers_zeroes_over_http() {
    let (status, body) = Book::new()
        .call(Method::GET, "/risk-dashboard/co-empty", json!({}))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["total_risks"], json!(0));
    assert_eq!(body["top_risks"], json!([]));
}

#[tokio::test]
async fn a_risk_in_a_category_that_is_not_one_of_the_eight_is_refused() {
    let (status, body) = post(
        "/risks",
        json!({"company_id": "co-1", "category": "weather", "name": "Rain"}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
}
