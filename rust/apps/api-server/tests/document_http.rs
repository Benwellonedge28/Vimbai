#![forbid(unsafe_code)]
//! document-service answers at the prefix it publishes.
//!
//! The filing register itself always lived in the documents crate; these tests pin that the addresses the service
//! published answer, and that they answer from that one implementation: a filing made, read back, searched for, counted,
//! and attached to the entity it belongs to.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use serde_json::{json, Value};
use store::fake::MemoryGraph;
use tower::ServiceExt;

async fn call(
    graph: Arc<MemoryGraph>,
    method: Method,
    uri: &str,
    payload: Option<Value>,
) -> (StatusCode, Value) {
    let router = vimbai_api::records::router(graph);
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .header("x-user-id", "user-1")
        .header("x-book-id", "book-1");
    let body = match payload {
        Some(value) => Body::from(value.to_string()),
        None => Body::empty(),
    };
    let response = router.oneshot(request.body(body).unwrap()).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 2_000_000)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[tokio::test]
async fn a_filing_is_made_and_read_back_at_the_published_addresses() {
    let graph = Arc::new(MemoryGraph::new());
    let (status, made) = call(
        Arc::clone(&graph),
        Method::POST,
        "/document/documents",
        Some(json!({
            "file_name": "invoice-2026-03.pdf",
            "document_type": "invoice",
            "title": "March stationery invoice",
            "tags": ["supplier", "march"],
            "file_size": 2048,
            "mime_type": "application/pdf",
            "content_base64": "JVBERi0xLjQK",
            "today": "2026-11-14",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{made}");
    let id = made["id"].as_str().expect("the filing's id").to_owned();

    let (status, held) = call(
        Arc::clone(&graph),
        Method::GET,
        &format!("/document/documents/{id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{held}");
    assert_eq!(held["document_type"], "invoice", "{held}");

    let (status, listed) = call(Arc::clone(&graph), Method::GET, "/document/documents", None).await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    assert!(
        listed.to_string().contains("invoice-2026-03.pdf"),
        "the filing is in the register: {listed}"
    );
}

#[tokio::test]
async fn a_search_narrows_the_register_and_the_statistics_count_it() {
    let graph = Arc::new(MemoryGraph::new());
    // one filing per document, with its own bytes: the register refuses the same content filed twice
    for (name, kind, content) in [
        ("lease.pdf", "contract", "JVBERi0xLjQK"),
        ("fuel-receipt.jpg", "receipt", "/9j/4AAQSkZJ"),
    ] {
        let (status, made) = call(
            Arc::clone(&graph),
            Method::POST,
            "/document/documents",
            Some(json!({
                "file_name": name,
                "document_type": kind,
                "title": name,
                "file_size": 100,
                "content_base64": content,
                "today": "2026-11-14",
            })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{made}");
    }

    let (status, found) = call(
        Arc::clone(&graph),
        Method::POST,
        "/document/documents/search",
        Some(json!({"document_type": "contract"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{found}");
    assert!(found.to_string().contains("lease.pdf"), "{found}");
    assert!(
        !found.to_string().contains("fuel-receipt.jpg"),
        "the receipt was not asked for: {found}"
    );

    let (status, counted) = call(
        Arc::clone(&graph),
        Method::GET,
        "/document/statistics",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{counted}");
    assert_eq!(counted["total_documents"], 2, "{counted}");
}

#[tokio::test]
async fn a_filing_is_attached_to_the_entity_it_belongs_to() {
    let graph = Arc::new(MemoryGraph::new());
    let (_, made) = call(
        Arc::clone(&graph),
        Method::POST,
        "/document/documents",
        Some(json!({
            "file_name": "po-77.pdf",
            "document_type": "purchase_order",
            "title": "Purchase order 77",
            "file_size": 400,
            "content_base64": "AA==",
            "today": "2026-11-14",
        })),
    )
    .await;
    let id = made["id"].as_str().expect("the filing's id").to_owned();

    let (status, linked) = call(
        Arc::clone(&graph),
        Method::POST,
        &format!("/document/documents/{id}/link"),
        Some(json!({"entity_type": "purchase_order", "entity_id": "po-77"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{linked}");

    let (_, held) = call(
        graph,
        Method::GET,
        &format!("/document/documents/{id}"),
        None,
    )
    .await;
    assert_eq!(held["linked_entity_type"], "purchase_order", "{held}");
    assert_eq!(held["linked_entity_id"], "po-77", "{held}");
}
