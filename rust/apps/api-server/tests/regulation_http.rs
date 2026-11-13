//! regulatory-compliance-service over HTTP: a regulation recorded, listed, moved on, and the board it adds up to.

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

async fn a_regulation(book: &Book, name: &str, framework: &str, risk: &str) -> Value {
    let (status, made) = book
        .call(
            Method::POST,
            "/regulations",
            json!({
                "company_id": "co-1",
                "regulation_name": name,
                "jurisdiction": "ZW",
                "framework": framework,
                "requirement": "File on time",
                "risk_if_non_compliant": risk
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{made}");
    made
}

#[tokio::test]
async fn a_regulation_is_recorded_and_listed_narrowed_by_framework() {
    let book = Book::new();
    let made = a_regulation(&book, "AML filing", "AML", "high").await;
    assert_eq!(made["status"], json!("pending_review"));
    a_regulation(&book, "IFRS 9", "IFRS", "medium").await;

    let (status, all) = book.get("/regulations/co-1").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(all.as_array().map(|held| held.len()), Some(2));

    let (status, narrowed) = book.get("/regulations/co-1?framework=AML").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(narrowed.as_array().map(|held| held.len()), Some(1));
    assert_eq!(narrowed[0]["regulation_name"], json!("AML filing"));
}

#[tokio::test]
async fn a_state_change_is_taken_and_an_unknown_state_is_ignored() {
    let book = Book::new();
    let made = a_regulation(&book, "AML filing", "AML", "high").await;
    let id = made["id"].as_str().expect("an id").to_string();

    let (status, changed) = book
        .call(
            Method::POST,
            &format!("/regulations/{id}/update"),
            json!({"company_id": "co-1", "status": "compliant", "today": "2026-03-01"}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{changed}");
    assert_eq!(changed["status"], json!("compliant"));

    let (status, nonsense) = book
        .call(
            Method::POST,
            &format!("/regulations/{id}/update"),
            json!({"company_id": "co-1", "status": "nonsense"}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{nonsense}");
    assert_eq!(nonsense["status"], json!("compliant"));
}

#[tokio::test]
async fn another_books_regulation_is_missing() {
    let book = Book::new();
    let made = a_regulation(&book, "AML filing", "AML", "high").await;
    let id = made["id"].as_str().expect("an id").to_string();
    let request = Request::builder()
        .method(Method::POST)
        .uri(format!("/regulations/{id}/update"))
        .header("content-type", "application/json")
        .header("x-user-id", "user-2")
        .header("x-book-id", "book-2")
        .body(Body::from(
            json!({"company_id": "co-1", "status": "compliant"}).to_string(),
        ))
        .unwrap();
    let (status, _) = book.answer(request).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn the_board_adds_up_the_regulations() {
    let book = Book::new();
    let made = a_regulation(&book, "AML filing", "AML", "high").await;
    a_regulation(&book, "PCI", "PCI_DSS", "critical").await;
    let id = made["id"].as_str().expect("an id").to_string();
    let (status, _) = book
        .call(
            Method::POST,
            &format!("/regulations/{id}/update"),
            json!({"company_id": "co-1", "status": "compliant", "today": "2026-03-01"}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    let (status, board) = book.get("/regulation-dashboard?company_id=co-1").await;
    assert_eq!(status, StatusCode::OK, "{board}");
    assert_eq!(board["total_regulations"], json!(2));
    assert_eq!(board["compliant"], json!(1));
    assert_eq!(board["compliance_rate"], json!(50.0));
    assert_eq!(board["by_framework"]["AML"]["compliant"], json!(1));
    // the settled AML no longer counts as critical; the pending critical PCI does
    assert_eq!(
        board["critical_items"].as_array().map(|held| held.len()),
        Some(1)
    );
    assert_eq!(board["critical_items"][0]["regulation"], json!("PCI"));
}

#[tokio::test]
async fn a_company_with_nothing_recorded_reads_a_hundred() {
    let book = Book::new();
    let (status, board) = book.get("/regulation-dashboard?company_id=co-9").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(board["total_regulations"], json!(0));
    assert_eq!(board["compliance_rate"], json!(100.0));
}
