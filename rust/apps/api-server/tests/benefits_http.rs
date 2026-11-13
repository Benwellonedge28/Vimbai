//! benefits-admin-service over HTTP: a plan offered, a person enrolled, and leave carried forward.

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

    async fn get(&self, uri: &str) -> (StatusCode, Value) {
        let request = Request::builder()
            .method(Method::GET)
            .uri(uri)
            .header("x-user-id", "user-1")
            .header("x-book-id", "book-1")
            .body(Body::empty())
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

async fn a_plan(book: &Book) -> Value {
    let (status, made) = book
        .call(
            Method::POST,
            "/benefit-plans",
            json!({
                "name": "Medical Aid",
                "plan_type": "medical",
                "employer_contribution_pct": 7.5,
                "employee_contribution_pct": 2.5,
                "eligibility_months": 3,
                "created_at": "2026-01-01T09:00:00Z"
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{made}");
    made
}

#[tokio::test]
async fn a_plan_is_offered_and_an_unknown_kind_is_refused() {
    let book = Book::new();
    let made = a_plan(&book).await;
    assert_eq!(made["status"], json!("active"));

    let (status, listed) = book.get("/benefit-plans").await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    assert_eq!(listed.as_array().map(|held| held.len()), Some(1));

    let (status, _) = book
        .call(
            Method::POST,
            "/benefit-plans",
            json!({"name": "Gym", "plan_type": "gym_membership"}),
        )
        .await;
    // the shipped answer was a validation refusal, which is a 422
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn a_person_is_enrolled_once_and_listed() {
    let book = Book::new();
    let plan = a_plan(&book).await;
    let plan_id = plan["id"].as_str().expect("an id");
    let asked = json!({"employee_id": "emp-1", "plan_id": plan_id, "beneficiary": "Tariro Moyo"});

    let (status, taken) = book.call(Method::POST, "/enroll", asked.clone()).await;
    assert_eq!(status, StatusCode::OK, "{taken}");

    let (status, _) = book.call(Method::POST, "/enroll", asked).await;
    assert_eq!(status, StatusCode::CONFLICT);

    let (status, listed) = book.get("/employee/emp-1/enrollments").await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    assert_eq!(listed.as_array().map(|held| held.len()), Some(1));
}

#[tokio::test]
async fn leave_carries_forward_and_survives_the_round_trip() {
    let book = Book::new();
    let (status, first) = book
        .call(
            Method::POST,
            "/leave/accrue",
            json!({
                "employee_id": "emp-1",
                "leave_type": "annual",
                "period": "2026-01",
                "accrued_days": 2.5,
                "taken_days": 0.5,
                "created_at": "2026-01-31T09:00:00Z"
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{first}");
    assert_eq!(first["balance_days"], json!(2.0));

    let (status, second) = book
        .call(
            Method::POST,
            "/leave/accrue",
            json!({
                "employee_id": "emp-1",
                "leave_type": "annual",
                "period": "2026-02",
                "accrued_days": 2.5,
                "taken_days": 1.25,
                "created_at": "2026-02-28T09:00:00Z"
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{second}");
    assert_eq!(second["balance_days"], json!(3.25));

    let (status, listed) = book.get("/employee/emp-1/leave?leave_type=annual").await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    assert_eq!(listed.as_array().map(|held| held.len()), Some(2));
    assert_eq!(listed[0]["period"], json!("2026-02"));
}
