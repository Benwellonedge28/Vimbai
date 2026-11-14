#![forbid(unsafe_code)]
//! mfa-auth-service over HTTP: enrolling a second factor, verifying a code, and spending a challenge.

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

#[tokio::test]
async fn a_second_factor_is_enrolled_checked_and_challenged() {
    let book = Book::new();
    let (status, made) = book
        .call(
            Method::POST,
            "/mfa/setup",
            json!({"user_id": "user-1", "now": "2026-01-01T00:00:00Z"}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{made}");
    let secret = made["secret"].as_str().expect("a secret").to_string();
    assert_eq!(made["backup_codes"].as_array().map(Vec::len), Some(8));
    assert!(made["qr_uri"].as_str().unwrap().contains(&secret));

    // enrolling on someone else's behalf is refused outright
    let (status, refused) = book
        .call(Method::POST, "/mfa/setup", json!({"user_id": "user-2"}))
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{refused}");

    let (status, passed) = book
        .call(
            Method::POST,
            "/mfa/verify",
            json!({"user_id": "user-1", "code": "123456", "now": "2026-01-01T00:01:00Z"}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{passed}");
    assert_eq!(passed["verified"], json!(true));

    let (status, failed) = book
        .call(Method::POST, "/mfa/verify", json!({"code": "12345"}))
        .await;
    assert_eq!(status, StatusCode::OK, "{failed}");
    assert_eq!(failed["verified"], json!(false));

    let (status, counted) = book.get("/mfa/status").await;
    assert_eq!(status, StatusCode::OK, "{counted}");
    assert_eq!(counted["enrolled_users"], json!(1));

    // a challenge is raised for the caller and spent once, and a wrong code does not spend it
    let (status, raised) = book
        .call(
            Method::POST,
            "/mfa/challenge?user_id=user-1&method=sms",
            json!({"now": "2026-01-01T00:00:00Z", "now_plus_five": "2026-01-01T00:05:00Z"}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{raised}");
    assert_eq!(raised["method"], json!("sms"));
    assert_eq!(raised["expires_in_minutes"], json!(5));
    let challenge_id = raised["challenge_id"].as_str().expect("an id").to_string();

    let (status, wrong) = book
        .call(
            Method::POST,
            &format!("/mfa/challenge/{challenge_id}/verify"),
            json!({"code": "000000", "now": "2026-01-01T00:01:00Z"}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{wrong}");
    assert_eq!(wrong["verified"], json!(false));

    // an expired challenge is dropped rather than answered
    let (status, expired) = book
        .call(
            Method::POST,
            &format!("/mfa/challenge/{challenge_id}/verify"),
            json!({"code": "000000", "now": "2026-01-01T00:06:00Z"}),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{expired}");

    let (status, spent) = book
        .call(
            Method::POST,
            &format!("/mfa/challenge/{challenge_id}/verify"),
            json!({"code": "000000", "now": "2026-01-01T00:07:00Z"}),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{spent}");

    let (status, backup) = book.get("/mfa/backup-codes").await;
    assert_eq!(status, StatusCode::OK, "{backup}");
    assert_eq!(backup["remaining"], json!(8));
}
