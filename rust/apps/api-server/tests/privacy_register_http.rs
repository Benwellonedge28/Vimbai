#![forbid(unsafe_code)]
//! privacy-admin-dashboard-service over HTTP: subject requests, consent, and the register's totals.

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use serde_json::{json, Value};
use std::sync::Arc;
use store::fake::MemoryGraph;
use tower::ServiceExt;

fn router() -> axum::Router {
    vimbai_api::records::router(Arc::new(MemoryGraph::new()))
}

async fn call(
    router: &axum::Router,
    method: Method,
    uri: &str,
    payload: Value,
) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .header("x-user-id", "user-1")
        .header("x-book-id", "book-1")
        .body(Body::from(payload.to_string()))
        .unwrap();
    let response = router.clone().oneshot(request).await.unwrap();
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
async fn a_subject_request_is_raised_settled_and_deleted() {
    let router = router();

    let (status, made) = call(
        &router,
        Method::POST,
        "/requests",
        json!({"request_type": "access", "subject_name": "Tariro Moyo",
               "subject_email": "tariro@example.test", "notes": "asked by phone"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{made}");
    assert_eq!(made["status"], json!("pending"));
    assert_eq!(made["notes"], json!("asked by phone"));
    let id = made["id"].as_str().expect("an id").to_string();

    // a fifth kind is a bad request, and a request with no kind is refused by the reader
    let (status, _) = call(
        &router,
        Method::POST,
        "/requests",
        json!({"request_type": "erasure", "subject_email": "a@b.test"}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // settling stamps the moment, and the listing narrows by status
    let (status, settled) = call(
        &router,
        Method::PUT,
        &format!("/requests/{id}"),
        json!({"status": "resolved", "notes": "pack sent"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{settled}");
    assert_eq!(settled["status"], json!("resolved"));
    assert!(!settled["resolved_at"].is_null());
    assert_eq!(settled["notes"], json!("pack sent"));
    let (_, pending) = call(&router, Method::GET, "/requests?status=pending", json!({})).await;
    assert_eq!(pending.as_array().map(Vec::len), Some(0));
    let (_, resolved) = call(&router, Method::GET, "/requests?status=resolved", json!({})).await;
    assert_eq!(resolved.as_array().map(Vec::len), Some(1));
    let (status, _) = call(
        &router,
        Method::PUT,
        "/requests/nope",
        json!({"status": "resolved"}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, gone) = call(
        &router,
        Method::DELETE,
        &format!("/requests/{id}"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{gone}");
    assert_eq!(gone["deleted"], json!(true));
    let (status, _) = call(
        &router,
        Method::DELETE,
        &format!("/requests/{id}"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn consent_is_granted_looked_up_and_withdrawn() {
    let router = router();

    let (status, given) = call(
        &router,
        Method::POST,
        "/consent",
        json!({"subject_email": "tariro@example.test", "data_type": "marketing"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{given}");
    assert_eq!(given["consent_given"], json!(true));
    assert_eq!(given["revoked_at"], Value::Null);
    let id = given["id"].as_str().expect("an id").to_string();

    // an explicit refusal is stored as such
    let (_, refused) = call(
        &router,
        Method::POST,
        "/consent",
        json!({"subject_email": "tariro@example.test", "data_type": "third_party",
               "consent_given": false}),
    )
    .await;
    assert_eq!(refused["consent_given"], json!(false));

    // looking up by email answers what still stands
    let (status, held) = call(
        &router,
        Method::GET,
        "/consent/tariro@example.test",
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{held}");
    assert_eq!(held.as_array().map(Vec::len), Some(2));

    // withdrawing keeps the record, turns the flag off, and takes it out of the answer
    let (status, gone) = call(
        &router,
        Method::DELETE,
        &format!("/consent/{id}"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{gone}");
    assert_eq!(gone["revoked"], json!(true));
    let (_, after) = call(
        &router,
        Method::GET,
        "/consent/tariro@example.test",
        json!({}),
    )
    .await;
    assert_eq!(after.as_array().map(Vec::len), Some(1));
    let (status, _) = call(&router, Method::DELETE, "/consent/nope", json!({})).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // the register adds up, and a rejection is neither pending nor resolved
    let (_, made) = call(
        &router,
        Method::POST,
        "/requests",
        json!({"request_type": "correction", "subject_email": "tariro@example.test"}),
    )
    .await;
    let refused_id = made["id"].as_str().expect("an id").to_string();
    let (_, _) = call(
        &router,
        Method::PUT,
        &format!("/requests/{refused_id}"),
        json!({"status": "rejected"}),
    )
    .await;
    let (status, counted) = call(&router, Method::GET, "/dashboard", json!({})).await;
    assert_eq!(status, StatusCode::OK, "{counted}");
    assert_eq!(counted["total_requests"], json!(1));
    assert_eq!(counted["pending_requests"], json!(0));
    assert_eq!(counted["resolved_requests"], json!(0));
    assert_eq!(counted["active_consents"], json!(0));
    assert_eq!(counted["revoked_consents"], json!(1));
}
