//! pos-integration-service over HTTP: a till, the takings it reports, and the summary those takings add up to.

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
async fn a_till_reports_its_takings_and_the_summary_adds_them_up() {
    let router = router();

    // a till is registered offline, and only once
    let (status, device) = call(
        &router,
        Method::POST,
        "/devices",
        json!({"device_id": "till-1", "device_name": "Front till", "device_type": "terminal"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{device}");
    assert_eq!(device["status"], json!("offline"));
    let (status, again) = call(
        &router,
        Method::POST,
        "/devices",
        json!({"device_id": "till-1", "device_name": "Front till", "device_type": "terminal"}),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{again}");

    // a state change is stored as given, because the shipped endpoint took a plain map
    let (status, _) = call(
        &router,
        Method::PUT,
        "/devices/till-1/status",
        json!({"status": "online"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_, held) = call(&router, Method::GET, "/devices/till-1", json!({})).await;
    assert_eq!(held["status"], json!("online"));
    let (status, _) = call(&router, Method::GET, "/devices/till-9", json!({})).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // takings arrive pending, and a replayed one is a conflict rather than a second record
    let (status, made) = call(
        &router,
        Method::POST,
        "/till-transactions",
        json!({"transaction_id": "tx-1", "device_id": "till-1", "transaction_type": "sale",
               "total_amount": 100, "payment_method": "cash", "timestamp": "2026-11-13T09:00:00Z"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{made}");
    assert_eq!(made["sync_status"], json!("pending"));
    let (status, _) = call(
        &router,
        Method::POST,
        "/till-transactions",
        json!({"transaction_id": "tx-1", "device_id": "till-1", "transaction_type": "sale",
               "total_amount": 100, "payment_method": "cash"}),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);

    // a batch answers one line at a time
    let (status, batch) = call(
        &router,
        Method::POST,
        "/till-transactions/batch",
        json!([
            {"transaction_id": "tx-1", "device_id": "till-1", "transaction_type": "sale", "total_amount": 10,
             "payment_method": "cash"},
            {"transaction_id": "tx-2", "device_id": "till-1", "transaction_type": "refund", "total_amount": 20.25,
             "payment_method": "card", "timestamp": "2026-11-13T10:00:00Z"}
        ]),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{batch}");
    assert_eq!(batch["results"][0]["status"], json!("rejected"));
    assert_eq!(batch["results"][1]["status"], json!("accepted"));

    // an outside system reporting in cents is read as money
    let (status, square) = call(
        &router,
        Method::POST,
        "/integrations/square/webhook",
        json!({"id": "sq-1", "location_id": "till-1", "total_money": {"amount": 4132}, "tax_money": {"amount": 132}}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{square}");
    assert_eq!(square["transaction_id"], json!("sq-1"));

    // the summary nets refunds and voids off the sales, over one span of one till
    let (status, summary) = call(
        &router,
        Method::POST,
        "/reports/sales-summary",
        json!({"device_id": "till-1", "start_date": "2000-01-01T00:00:00Z", "end_date": "2100-01-01T00:00:00Z"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{summary}");
    assert_eq!(summary["total_transactions"], json!(3));
    assert_eq!(summary["total_sales"], json!(141.32));
    assert_eq!(summary["total_refunds"], json!(20.25));
    assert_eq!(summary["net_sales"], json!(121.07));
    assert_eq!(summary["by_payment_method"]["card"], json!(61.57));

    // marking one synced does not pretend a journal entry exists
    let (status, synced) = call(
        &router,
        Method::POST,
        "/till-transactions/tx-1/sync",
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{synced}");
    assert_eq!(synced["sync_status"], json!("synced"));
    assert_eq!(synced["journal_posted"], json!(false));

    // the metrics and the two inventory doors answer over the same records
    let (status, metrics) = call(&router, Method::GET, "/pos-metrics", json!({})).await;
    assert_eq!(status, StatusCode::OK, "{metrics}");
    assert_eq!(metrics["registered_devices"], json!(1));
    assert_eq!(metrics["synced"], json!(1));
    assert_eq!(metrics["connected_devices"], json!(0));
    let (_, synced_stock) = call(
        &router,
        Method::POST,
        "/inventory/sync",
        json!({"device_id": "till-1", "products": [{"sku": "A"}]}),
    )
    .await;
    assert_eq!(synced_stock["recorded"], json!(false));
    let (_, reconciled) = call(
        &router,
        Method::POST,
        "/inventory/reconcile",
        json!({"device_id": "till-1", "inventory_data": [{"sku": "A"}]}),
    )
    .await;
    assert_eq!(reconciled["discrepancies_found"], json!(0));
}
