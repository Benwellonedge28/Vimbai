//! zero-trust-data-service over HTTP: a policy set, a decision taken against it, and the decisions on record.

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
async fn a_policy_is_set_and_a_decision_is_taken_against_it() {
    let book = Book::new();
    let (status, policy) = book
        .call(
            Method::POST,
            "/access-policies",
            json!({
                "resource": "ledger",
                "required_roles": ["clerk"],
                "required_clearance": "internal",
                "mfa_required": false,
                "created_at": "2026-01-01T09:00:00Z"
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{policy}");

    let (status, allowed) = book
        .call(
            Method::POST,
            "/evaluate",
            json!({
                "user_id": "person-1",
                "resource": "ledger",
                "user_roles": ["clerk"],
                "user_clearance": "confidential",
                "timestamp": "2026-01-02T09:00:00Z"
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{allowed}");
    assert_eq!(allowed["granted"], json!(true));
    assert_eq!(allowed["policy_id"], policy["id"]);

    let (status, refused_decision) = book
        .call(
            Method::POST,
            "/evaluate",
            json!({
                "user_id": "person-2",
                "resource": "ledger",
                "user_roles": ["intern"],
                "user_clearance": "public",
                "timestamp": "2026-01-03T09:00:00Z"
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{refused_decision}");
    assert_eq!(refused_decision["granted"], json!(false));
    assert_eq!(
        refused_decision["reason"],
        json!("Missing required role; Insufficient clearance")
    );
}

#[tokio::test]
async fn decisions_are_listed_and_another_books_policies_are_not_reachable() {
    let book = Book::new();
    let (_, policy) = book
        .call(
            Method::POST,
            "/access-policies",
            json!({"resource": "payroll", "mfa_required": false, "created_at": "2026-01-01T09:00:00Z"}),
        )
        .await;
    let policy_id = policy["id"].as_str().expect("an id");

    let (status, decided) = book
        .call(
            Method::POST,
            "/evaluate",
            json!({"user_id": "person-1", "resource": "payroll", "timestamp": "2026-01-02T09:00:00Z"}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{decided}");
    assert_eq!(decided["granted"], json!(true), "no roles, no MFA asked");

    let (status, listed) = book.get("/attempts?limit=1").await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    assert_eq!(listed.as_array().map(|held| held.len()), Some(1));

    let (status, theirs) = book.get("/attempts/user/person-1").await;
    assert_eq!(status, StatusCode::OK, "{theirs}");
    assert_eq!(theirs.as_array().map(|held| held.len()), Some(1));

    let (status, replaced) = book
        .call(
            Method::PUT,
            &format!("/access-policies/{policy_id}"),
            json!({"resource": "treasury"}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{replaced}");
    assert_eq!(replaced["resource"], json!("treasury"));
    assert_eq!(replaced["mfa_required"], json!(true));

    let (status, gone) = book
        .call(
            Method::DELETE,
            &format!("/access-policies/{policy_id}"),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{gone}");
    assert_eq!(gone["deleted"], json!(true));

    let (status, again) = book
        .call(
            Method::DELETE,
            &format!("/access-policies/{policy_id}"),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{again}");
}
