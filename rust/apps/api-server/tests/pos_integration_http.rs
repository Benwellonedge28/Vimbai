#![forbid(unsafe_code)]
//! pos-integration answers at the prefix it publishes.
//!
//! The till itself always lived in the payments crate; these tests pin that the addresses the service published answer,
//! and that they answer from that one implementation: a terminal registered, a taking received and read back, a queue
//! emptied in a batch, and the metrics the counters read.

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

fn terminal() -> Value {
    json!({
        "device_id": "till-1",
        "device_name": "Front counter till",
        "device_type": "tablet",
        "location_id": "shop-1",
        "today": "2026-11-14",
    })
}

fn taking(id: &str, amount: &str) -> Value {
    json!({
        "transaction_id": id,
        "device_id": "till-1",
        "transaction_type": "sale",
        "total_amount": amount,
        "tax_amount": "0.00",
        "discount_amount": "0.00",
        "payment_method": "card",
        "items": [{"sku": "A-1", "quantity": 1}],
        "today": "2026-11-14",
    })
}

#[tokio::test]
async fn a_terminal_is_registered_and_a_taking_read_back_at_the_published_addresses() {
    let graph = Arc::new(MemoryGraph::new());
    let (status, device) = call(
        Arc::clone(&graph),
        Method::POST,
        "/pos-integration/devices",
        Some(terminal()),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{device}");
    assert_eq!(
        device["status"], "offline",
        "a new till has never spoken: {device}"
    );

    let (status, listed) = call(
        Arc::clone(&graph),
        Method::GET,
        "/pos-integration/devices",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    assert!(listed.to_string().contains("till-1"), "{listed}");

    let (status, taken) = call(
        Arc::clone(&graph),
        Method::POST,
        "/pos-integration/transactions",
        Some(taking("rec-1", "125.50")),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{taken}");

    let (status, held) = call(
        Arc::clone(&graph),
        Method::GET,
        "/pos-integration/transactions/rec-1",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{held}");
    assert_eq!(held["total_amount"], "125.50", "{held}");
    assert_eq!(held["sync_status"], "pending", "{held}");
}

#[tokio::test]
async fn a_queue_is_emptied_in_one_batch_and_a_repeat_is_refused() {
    let graph = Arc::new(MemoryGraph::new());
    let (_, device) = call(
        Arc::clone(&graph),
        Method::POST,
        "/pos-integration/devices",
        Some(terminal()),
    )
    .await;
    assert_eq!(device["device_id"], "till-1");

    let (status, emptied) = call(
        Arc::clone(&graph),
        Method::POST,
        "/pos-integration/transactions/batch",
        Some(json!([
            taking("rec-10", "40.00"),
            taking("rec-11", "60.00")
        ])),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{emptied}");
    assert_eq!(emptied["total"], 2, "{emptied}");
    assert_eq!(emptied["results"][0]["status"], "accepted", "{emptied}");
    assert_eq!(emptied["results"][1]["status"], "accepted", "{emptied}");

    // the same till sending the same receipt twice is one receipt
    let (status, again) = call(
        Arc::clone(&graph),
        Method::POST,
        "/pos-integration/transactions",
        Some(taking("rec-10", "40.00")),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{again}");

    let (status, metrics) = call(
        Arc::clone(&graph),
        Method::GET,
        "/pos-integration/metrics",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{metrics}");
    assert_eq!(metrics["total_transactions"], 2, "{metrics}");
}

#[tokio::test]
async fn a_taking_is_marked_synced_once_the_ledger_has_it() {
    let graph = Arc::new(MemoryGraph::new());
    call(
        Arc::clone(&graph),
        Method::POST,
        "/pos-integration/devices",
        Some(terminal()),
    )
    .await;
    call(
        Arc::clone(&graph),
        Method::POST,
        "/pos-integration/transactions",
        Some(taking("rec-20", "12.00")),
    )
    .await;

    let (status, synced) = call(
        Arc::clone(&graph),
        Method::POST,
        "/pos-integration/transactions/rec-20/sync",
        Some(json!({"journal_entry_id": "je-99", "today": "2026-11-14"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{synced}");

    // the till says it has been taken up; this service records the taking, it does not post to the ledger
    assert_eq!(synced["sync_status"], "synced", "{synced}");
    assert_eq!(synced["journal_posted"], false, "{synced}");

    let (_, held) = call(
        graph,
        Method::GET,
        "/pos-integration/transactions/rec-20",
        None,
    )
    .await;
    assert_eq!(held["sync_status"], "synced", "{held}");
}
