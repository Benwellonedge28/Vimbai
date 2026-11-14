#![forbid(unsafe_code)]
//! Cursor pagination, over a real listing and over the two shapes a collection arrives in.
//!
//! The convention had to land in v1 without changing the shape of a list, so it is opt-in by request: a caller who sends
//! no paging parameters gets what they always got. These tests hold both halves - the answer is untouched when nobody asks,
//! and a caller who asks gets a page they can walk to the end without skipping or repeating a row.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use axum::routing::get as route_get;
use axum::{Json, Router};
use serde_json::{json, Value};
use store::fake::MemoryGraph;
use tower::ServiceExt;

async fn get(router: Router, uri: &str) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(Method::GET)
        .uri(uri)
        .header("x-user-id", "user-1")
        .header("x-book-id", "book-1")
        .body(Body::empty())
        .unwrap();
    let response = router.oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 8 * 1024 * 1024)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

/// A router serving the two shapes a collection is answered in: a bare array, and rows under a name beside a count.
fn fixture() -> Router {
    let rows = || {
        json!([
            {"id": "a", "name": "VAT"},
            {"id": "b", "name": "Withholding"},
            {"id": "c", "name": "Excise"},
        ])
    };
    let listed = rows();
    let named = rows();
    let sequenced = rows();
    let single = rows()[0].clone();
    Router::new()
        .route(
            "/bare",
            route_get(move || {
                let listed = listed.clone();
                async move { Json(listed) }
            }),
        )
        .route(
            "/named",
            route_get(move || {
                let named = named.clone();
                async move { Json(json!({ "rates": named, "count": 3 })) }
            }),
        )
        .route(
            // a listing that carries something a reader needs beside its rows, which is what a Book's entries do
            "/sequenced",
            route_get(move || {
                let sequenced = sequenced.clone();
                async move { Json(json!({ "entries": sequenced, "book_seq": 42 })) }
            }),
        )
        .route(
            "/one",
            route_get(move || {
                let single = single.clone();
                async move { Json(single) }
            }),
        )
}

fn guarded() -> Router {
    vimbai_api::pagination::guard(fixture())
}

#[tokio::test]
async fn a_caller_who_does_not_ask_for_a_page_gets_what_they_always_got() {
    let (_, bare) = get(guarded(), "/bare").await;
    assert!(bare.is_array(), "an array stays an array: {bare}");
    assert_eq!(bare.as_array().map(Vec::len), Some(3));
    let (_, named) = get(guarded(), "/named").await;
    assert_eq!(named["rates"].as_array().map(Vec::len), Some(3), "{named}");
    assert!(
        named.get("items").is_none(),
        "no envelope unless it was asked for"
    );
}

#[tokio::test]
async fn asking_for_a_page_answers_items_and_a_cursor() {
    let (status, body) = get(guarded(), "/bare?limit=2").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["items"].as_array().map(Vec::len), Some(2));
    assert_eq!(body["has_more"], json!(true));
    assert!(body["next_cursor"].is_string(), "{body}");
    assert_eq!(body["total"], json!(3));
}

#[tokio::test]
async fn walking_the_cursor_hands_out_every_row_once() {
    let (_, whole) = get(guarded(), "/bare").await;
    let expected = whole.as_array().cloned().unwrap_or_default();
    let mut walked: Vec<Value> = Vec::new();
    let mut uri = "/bare?limit=1".to_string();
    for _ in 0..10 {
        let (status, body) = get(guarded(), &uri).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        walked.extend(body["items"].as_array().cloned().unwrap_or_default());
        match body["next_cursor"].as_str() {
            Some(cursor) if body["has_more"] == json!(true) => {
                uri = format!("/bare?limit=1&cursor={cursor}");
            }
            _ => break,
        }
    }
    assert_eq!(
        walked, expected,
        "the same rows, in the same order, once each"
    );
}

#[tokio::test]
async fn a_named_collection_keeps_its_name_and_gains_the_envelope() {
    let (status, body) = get(guarded(), "/named?limit=2").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["rates"].as_array().map(Vec::len), Some(2), "{body}");
    assert_eq!(body["items"].as_array().map(Vec::len), Some(2));
    assert_eq!(body["has_more"], json!(true));
    assert_eq!(body["count"], json!(2));
}

#[tokio::test]
async fn a_field_the_handler_put_beside_its_rows_survives_the_page() {
    // A device pulls a Book's entries and is told the Book's sequence, which is what it quotes back next time. An
    // envelope that kept only the rows would leave it unable to say where it had got to.
    let (status, body) = get(guarded(), "/sequenced?limit=2").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["entries"].as_array().map(Vec::len), Some(2), "{body}");
    assert_eq!(
        body["book_seq"],
        json!(42),
        "the handler's own field: {body}"
    );
    assert_eq!(body["count"], json!(2), "and the envelope's, beside it");
    assert_eq!(body["total"], json!(3));
}

#[tokio::test]
async fn an_answer_that_is_not_a_collection_passes_through_untouched() {
    let (status, body) = get(guarded(), "/one?limit=1").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["id"], json!("a"));
    assert!(body.get("items").is_none(), "{body}");
}

#[tokio::test]
async fn a_cursor_the_engine_did_not_give_out_is_refused() {
    let (status, body) = get(guarded(), "/bare?cursor=made-up").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
}

#[tokio::test]
async fn a_nonsense_limit_is_refused_rather_than_guessed_at() {
    let (status, body) = get(guarded(), "/bare?limit=0").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    let (status, body) = get(guarded(), "/bare?limit=many").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
}

#[tokio::test]
async fn the_layer_is_mounted_where_the_binaries_serve_and_leaves_a_list_alone() {
    // through the assembled engine: a guard nobody mounts protects nothing
    let graph: vimbai_api::records::GraphHandle = Arc::new(MemoryGraph::default());
    let (status, body) = get(vimbai_api::routed(Some(graph)), "/tax-rates").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        body.is_object(),
        "a list answers the shape it always did: {body}"
    );
    assert!(
        body.get("items").is_none(),
        "no envelope was asked for: {body}"
    );
}
