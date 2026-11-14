#![forbid(unsafe_code)]
//! encrypted-backup-service over HTTP: taking a backup, listing it, restoring from it and deleting it.

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
async fn a_backup_is_taken_restored_and_deleted() {
    let router = router();

    // a backup names a service and one of the three shapes, and carries a real digest
    let (status, made) = call(
        &router,
        Method::POST,
        "/backup",
        json!({"service_name": "accounting-service", "backup_type": "full", "created_by": "user-1"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{made}");
    assert_eq!(made["status"], json!("completed"));
    assert_eq!(made["size_bytes"], json!(0));
    let checksum = made["checksum"].as_str().expect("a digest").to_string();
    assert!(
        checksum.starts_with("sha256:") && checksum.len() == 71,
        "{checksum}"
    );
    assert!(!made["encryption_key_id"].as_str().unwrap_or("").is_empty());
    let id = made["id"].as_str().expect("an id").to_string();

    // a type nobody agreed on, and no service at all, are refused
    let (status, _) = call(
        &router,
        Method::POST,
        "/backup",
        json!({"service_name": "accounting-service", "backup_type": "partial"}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = call(
        &router,
        Method::POST,
        "/backup",
        json!({"backup_type": "full"}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    // the listing narrows by service and by status
    let (_, listed) = call(&router, Method::GET, "/backups", json!({})).await;
    assert_eq!(listed.as_array().map(Vec::len), Some(1));
    let (_, other) = call(
        &router,
        Method::GET,
        "/backups?service_name=ledger-service",
        json!({}),
    )
    .await;
    assert_eq!(other.as_array().map(Vec::len), Some(0));
    let (status, found) = call(&router, Method::GET, &format!("/backups/{id}"), json!({})).await;
    assert_eq!(status, StatusCode::OK, "{found}");
    assert_eq!(found["id"], json!(id));
    let (status, _) = call(&router, Method::GET, "/backups/nope", json!({})).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // a restore names the person who asked, and only a finished backup answers
    let (status, restored) = call(
        &router,
        Method::POST,
        &format!("/backups/{id}/restore"),
        json!({"restored_by": "user-1"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{restored}");
    assert_eq!(restored["backup_id"], json!(id));
    assert_eq!(restored["status"], json!("completed"));
    let (_, restores) = call(&router, Method::GET, "/restores", json!({})).await;
    assert_eq!(restores.as_array().map(Vec::len), Some(1));
    let (status, _) = call(
        &router,
        Method::POST,
        "/backups/nope/restore",
        json!({"restored_by": "user-1"}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // deleting answers what went, and a second delete finds nothing
    let (status, gone) = call(
        &router,
        Method::DELETE,
        &format!("/backups/{id}"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{gone}");
    assert_eq!(gone["deleted"], json!(true));
    let (status, _) = call(
        &router,
        Method::DELETE,
        &format!("/backups/{id}"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
