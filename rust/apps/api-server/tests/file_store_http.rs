//! The engine over a file instead of a database: what the on-device runtime boots.
//!
//! A record written through HTTP must be there when the store is opened again, because the machine an offline install
//! runs on has nothing to fall back on.

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use serde_json::{json, Value};
use std::sync::Arc;
use store::file::FileGraph;
use tower::ServiceExt;

async fn call(
    graph: Arc<FileGraph>,
    method: Method,
    uri: &str,
    payload: Value,
) -> (StatusCode, Value) {
    let router = vimbai_api::records::router(graph);
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

fn scratch() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("vimbai-file-http-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir.join("book.json")
}

#[tokio::test]
async fn an_invoice_raised_on_disk_is_still_there_after_the_engine_restarts() {
    let path = scratch();
    {
        let graph = Arc::new(FileGraph::open(&path).expect("open"));
        let (status, customer) = call(
            graph.clone(),
            Method::POST,
            "/customers",
            json!({"customer_id": "cus-1", "name": "Acme", "email": "billing@acme.test"}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{customer}");
        assert_eq!(customer["customer_id"], json!("cus-1"), "{customer}");
        let (status, made) = call(
            graph,
            Method::POST,
            "/invoices",
            json!({"invoice_number": "INV-9", "customer_id": "cus-1",
                   "items": [{"description": "work", "amount": "50.00"}], "total_amount": "50.00"}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{made}");
        // an invoice is addressed by its number, which is what a client has
        assert_eq!(made["invoice_number"], json!("INV-9"));
    }

    let restarted = Arc::new(FileGraph::open(&path).expect("reopen"));
    let (status, held) = call(restarted, Method::GET, "/invoices/INV-9", json!({})).await;
    assert_eq!(status, StatusCode::OK, "{held}");
    assert_eq!(held["invoice_number"], json!("INV-9"));
}
