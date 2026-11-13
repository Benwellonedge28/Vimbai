//! financial-identity-service over HTTP: a profile recorded for a subject, read back by id and by subject, and the
//! verification that moves its state.

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

async fn a_profile(book: &Book, subject: &str) -> Value {
    let (status, made) = book
        .call(
            Method::POST,
            "/profiles",
            json!({
                "user_id": subject,
                "legal_name": "Tariro Moyo",
                "national_id": "63-123456-A-00",
                "annual_income": 42000,
                "created_at": "2026-01-01T09:00:00Z"
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{made}");
    made
}

#[tokio::test]
async fn a_profile_is_recorded_for_its_subject_and_read_back() {
    let book = Book::new();
    let made = a_profile(&book, "user-9").await;
    assert_eq!(made["user_id"], json!("user-9"));
    assert_eq!(made["verification_status"], json!("pending"));
    let id = made["id"].as_str().expect("an id").to_string();

    let (status, fetched) = book.get(&format!("/profiles/{id}")).await;
    assert_eq!(status, StatusCode::OK, "{fetched}");
    assert_eq!(fetched["id"], json!(id));

    let (status, by_subject) = book.get("/profiles/by-subject/user-9").await;
    assert_eq!(status, StatusCode::OK, "{by_subject}");
    assert_eq!(by_subject["id"], json!(id));
}

#[tokio::test]
async fn two_documents_verify_the_profile_from_the_query_string() {
    let book = Book::new();
    let made = a_profile(&book, "user-9").await;
    let id = made["id"].as_str().expect("an id").to_string();

    // the shipped endpoint took its documents as repeated query parameters
    let request = Request::builder()
        .method(Method::PUT)
        .uri(format!(
            "/profiles/{id}/verify?documents=passport&documents=utility%20bill&verified_at=2026-03-02T10:00:00Z"
        ))
        .header("x-user-id", "user-1")
        .header("x-book-id", "book-1")
        .body(Body::empty())
        .unwrap();
    let (status, checked) = book.answer(request).await;
    assert_eq!(status, StatusCode::OK, "{checked}");
    assert_eq!(checked["status"], json!("verified"));
    assert_eq!(checked["risk_score"], json!(20));

    let (_, after) = book.get(&format!("/profiles/{id}")).await;
    assert_eq!(after["verification_status"], json!("verified"));
    assert_eq!(
        after["kyc_documents"],
        json!(["passport", "utility bill"]),
        "both documents are kept"
    );
}

#[tokio::test]
async fn one_document_scores_high_without_marking_the_profile() {
    let book = Book::new();
    let made = a_profile(&book, "user-9").await;
    let id = made["id"].as_str().expect("an id").to_string();
    let (status, checked) = book
        .call(
            Method::PUT,
            &format!("/profiles/{id}/verify"),
            json!({"documents": ["passport"]}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{checked}");
    assert_eq!(checked["status"], json!("pending"));
    assert_eq!(checked["risk_score"], json!(80));
}

#[tokio::test]
async fn another_books_profile_is_missing() {
    let book = Book::new();
    let made = a_profile(&book, "user-9").await;
    let id = made["id"].as_str().expect("an id").to_string();
    let request = Request::builder()
        .method(Method::GET)
        .uri(format!("/profiles/{id}"))
        .header("x-user-id", "user-2")
        .header("x-book-id", "book-2")
        .body(Body::empty())
        .unwrap();
    let (status, _) = book.answer(request).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, _) = book.get("/profiles/by-subject/user-404").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
