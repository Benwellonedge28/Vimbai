//! The addresses retired services published, checked against the engine that answers for them now.
//!
//! A service was retired only once its paths were answered, but the check that decided that counted fixed paths only.
//! A parameterised address can therefore have moved without anyone noticing, which is a 404 for a client that still
//! calls it. Each test here asks for one of those addresses and refuses a 404 in reply.

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

/// The address exists when a handler answers it. A handler may refuse the request - no such record, a bad body - but
/// an address nothing is mounted on answers with no error body at all, which is what tells the two apart.
async fn address_answers(method: Method, uri: &str, payload: Value) {
    let (status, body) = call(method, uri, payload).await;
    assert!(
        body["error"].is_string(),
        "{uri} reached no handler ({status}): {body}"
    );
    assert_ne!(status, StatusCode::METHOD_NOT_ALLOWED, "{uri}: {body}");
}

#[tokio::test]
async fn an_address_with_no_route_reaches_no_handler_at_all() {
    let (_, body) = call(Method::GET, "/no-such-address-anywhere", json!({})).await;
    assert!(body["error"].is_null(), "{body}");
}

#[tokio::test]
async fn a_bank_reconciliation_still_auto_matches_and_posts_its_adjustments() {
    for step in ["auto-match", "post-adjustments"] {
        address_answers(
            Method::POST,
            &format!("/reconcile/rec-1/{step}"),
            json!({"today": "2026-03-01T09:00:00Z"}),
        )
        .await;
    }
}

#[tokio::test]
async fn a_backup_can_still_be_restored_at_the_path_its_service_published() {
    address_answers(
        Method::POST,
        "/encrypted-backup/backup/bak-1/restore",
        json!({}),
    )
    .await;
}

#[tokio::test]
async fn a_profile_is_still_read_by_the_user_it_belongs_to() {
    address_answers(Method::GET, "/profiles/user/user-1", json!({})).await;
}

#[tokio::test]
async fn a_right_issue_is_still_accepted_and_renounced_where_it_was_published() {
    for decision in ["accept", "renounce"] {
        address_answers(
            Method::POST,
            &format!("/right-issues/issue-1/{decision}"),
            json!({"today": "2026-03-01T09:00:00Z"}),
        )
        .await;
    }
}
