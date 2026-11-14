#![forbid(unsafe_code)]
//! multimodal-pipeline-service over HTTP: a capture, the inbox by state, and a correction.

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
async fn a_capture_waits_in_the_inbox_until_someone_accepts_or_corrects_it() {
    let router = router();

    let (status, made) = call(
        &router,
        Method::POST,
        "/tasks",
        json!({"input_type": "document", "input_url": "https://files.test/receipt.jpg"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{made}");
    assert_eq!(made["status"], json!("received"));
    // nothing here read the document, and the answer says so rather than leaving a caller to poll
    assert_eq!(made["reading_queued"], json!(false));
    let id = made["id"].as_str().expect("an id").to_string();

    // a sixth kind of input is refused as unprocessable
    let (status, _) = call(
        &router,
        Method::POST,
        "/tasks",
        json!({"input_type": "hologram"}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    // the inbox holds what is not yet accepted
    let (status, open) = call(&router, Method::GET, "/inbox", json!({})).await;
    assert_eq!(status, StatusCode::OK, "{open}");
    assert_eq!(open.as_array().map(Vec::len), Some(1));

    // a fourth view is understood and refused, and the limit is held between one and five hundred
    let (status, _) = call(&router, Method::GET, "/inbox?view=everything", json!({})).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let (status, held) = call(&router, Method::GET, "/inbox?view=all&limit=0", json!({})).await;
    assert_eq!(status, StatusCode::OK, "{held}");
    assert_eq!(held.as_array().map(Vec::len), Some(1));

    // the badge counts each state
    let (status, summary) = call(&router, Method::GET, "/inbox/summary", json!({})).await;
    assert_eq!(status, StatusCode::OK, "{summary}");
    assert_eq!(summary["received"], json!(1));
    assert_eq!(summary["total_incomplete"], json!(1));
    assert_eq!(summary["total_organized"], json!(0));

    // a correction marks the capture corrected, and keeps the value it started from
    let (status, corrected) = call(
        &router,
        Method::POST,
        &format!("/tasks/{id}/corrections"),
        json!({"field_name": "total_amount", "original_value": "41.32", "corrected_value": "43.12"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{corrected}");
    assert_eq!(corrected["original_value"], json!("41.32"));
    let (_, after) = call(&router, Method::GET, &format!("/tasks/{id}"), json!({})).await;
    assert_eq!(after["status"], json!("user_corrected"));

    // accepting it twice changes nothing, keeping the first finishing time
    let (status, first) = call(
        &router,
        Method::POST,
        &format!("/inbox/{id}/organize"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{first}");
    assert_eq!(first["status"], json!("completed"));
    let first_end = first["processing_end_time"].clone();
    let (_, again) = call(
        &router,
        Method::POST,
        &format!("/inbox/{id}/organize"),
        json!({"now": 9999}),
    )
    .await;
    assert_eq!(again["processing_end_time"], first_end);

    // accepted records move to the organized view
    let (_, organized) = call(&router, Method::GET, "/inbox?view=organized", json!({})).await;
    assert_eq!(organized.as_array().map(Vec::len), Some(1));

    // a correction on a capture that is not there is missing
    let (status, _) = call(
        &router,
        Method::POST,
        "/tasks/nope/corrections",
        json!({"field_name": "x"}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // a delete removes it, and deleting again is missing
    let (status, gone) = call(&router, Method::DELETE, &format!("/tasks/{id}"), json!({})).await;
    assert_eq!(status, StatusCode::OK, "{gone}");
    let (status, _) = call(&router, Method::DELETE, &format!("/tasks/{id}"), json!({})).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
