//! document-service over HTTP: a file filed, refused when it is a second copy, searched by its own text, and deleted.

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

fn a_file(name: &str, content: &str, kind: &str, stamp: &str) -> Value {
    json!({
        "file_name": name,
        "title": format!("{name} filed"),
        "document_type": kind,
        "tags": "supplier,january",
        "content_base64": content,
        "file_size": content.len(),
        "created_at": stamp
    })
}

#[tokio::test]
async fn a_file_is_filed_searched_by_its_own_text_and_counted() {
    let book = Book::new();
    let (status, made) = book
        .call(
            Method::POST,
            "/files",
            a_file("invoice.pdf", "pages", "invoice", "2026-01-05T09:00:00Z"),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{made}");
    assert_eq!(made["mime_type"], json!("application/pdf"));

    // the same content is one document, not two
    let (status, refused) = book
        .call(
            Method::POST,
            "/files",
            a_file(
                "invoice-copy.pdf",
                "pages",
                "invoice",
                "2026-01-06T09:00:00Z",
            ),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{refused}");

    // a file of an unlisted kind never reaches the store
    let (status, _) = book
        .call(
            Method::POST,
            "/files",
            a_file("notes.txt", "words", "other", "2026-01-06T09:00:00Z"),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    let document_id = made["id"].as_str().expect("an id");
    let (status, read) = book
        .call(
            Method::POST,
            &format!("/files/{document_id}/ocr"),
            json!({"processed_at": "2026-01-07T09:00:00Z"}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{read}");
    assert_eq!(read["confidence"], json!(0.90));

    let (status, found) = book
        .call(
            Method::POST,
            "/files/search",
            json!({"query": "placeholder"}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{found}");
    assert_eq!(found["total"], json!(1));

    let (status, counted) = book.get("/document-statistics").await;
    assert_eq!(status, StatusCode::OK, "{counted}");
    assert_eq!(counted["total_documents"], json!(1));
    assert_eq!(counted["by_type"]["invoice"], json!(1));
}

#[tokio::test]
async fn a_picture_previews_and_a_document_is_deleted_then_cannot_be_handed_back() {
    let book = Book::new();
    let (status, picture) = book
        .call(
            Method::POST,
            "/files",
            a_file("receipt.png", "pixels", "receipt", "2026-01-05T09:00:00Z"),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{picture}");
    let picture_id = picture["id"].as_str().expect("an id");

    let (status, preview) = book.get(&format!("/files/{picture_id}/preview")).await;
    assert_eq!(status, StatusCode::OK, "{preview}");
    assert_eq!(preview["mime_type"], json!("image/png"));

    let (status, download) = book.get(&format!("/files/{picture_id}/download")).await;
    assert_eq!(status, StatusCode::OK, "{download}");
    assert_eq!(download["content_base64"], json!("pixels"));

    // a listing narrows by kind and by a tag, and leaves deleted documents out
    let (status, listed) = book.get("/files?document_type=receipt&tag=january").await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    assert_eq!(listed["total"], json!(1));

    let (status, gone) = book
        .call(
            Method::DELETE,
            &format!("/files/{picture_id}?deleted_at=2026-02-01T09:00:00Z"),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{gone}");
    assert_eq!(gone["permanent"], json!(false));

    let (status, after) = book.get("/files").await;
    assert_eq!(status, StatusCode::OK, "{after}");
    assert_eq!(after["total"], json!(0));

    let (status, again) = book.get(&format!("/files/{picture_id}/download")).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{again}");

    // another Book never sees it at all
    let other = Request::builder()
        .method(Method::GET)
        .uri(format!("/files/{picture_id}"))
        .header("x-user-id", "user-2")
        .header("x-book-id", "book-2")
        .body(Body::empty())
        .unwrap();
    let response = book.router.clone().oneshot(other).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
