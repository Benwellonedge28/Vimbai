//! invoicing-service over HTTP: a customer, an invoice against it, and the payment that settles it.

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

fn lines() -> Value {
    json!([{"description": "Consulting", "quantity": 2, "unit_price": 50, "amount": 100}])
}

#[tokio::test]
async fn an_invoice_is_raised_against_a_customer_and_settled_by_a_payment() {
    let router = router();

    let (status, made) = call(
        &router,
        Method::POST,
        "/customers",
        json!({"customer_id": "ACME-1", "name": "Acme Traders", "email": "billing@acme.test"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{made}");
    assert_eq!(made["customer_id"], json!("ACME-1"));

    // the identifier is the Book's own, so a second customer under it is a conflict
    let (status, _) = call(
        &router,
        Method::POST,
        "/customers",
        json!({"customer_id": "ACME-1", "name": "Acme Again"}),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);

    // an invoice bills a customer that exists here
    let (status, invoice) = call(
        &router,
        Method::POST,
        "/invoices",
        json!({"invoice_number": "INV-1", "customer_id": "ACME-1", "invoice_date": "2026-11-01",
               "due_date": "2026-11-30", "total_amount": 100, "items": lines(),
               "status": "outstanding"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{invoice}");
    assert_eq!(invoice["status"], json!("outstanding"));
    assert_eq!(invoice["paid_amount"], json!("0.000000"));

    let (status, _) = call(
        &router,
        Method::POST,
        "/invoices",
        json!({"invoice_number": "INV-2", "customer_id": "NOBODY", "total_amount": 100, "items": lines()}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // a total that does not match its lines is refused
    let (status, _) = call(
        &router,
        Method::POST,
        "/invoices",
        json!({"invoice_number": "INV-3", "customer_id": "ACME-1", "total_amount": 500, "items": lines()}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // part payment stands on the invoice without settling it, and says no journal was posted
    let (status, part) = call(
        &router,
        Method::POST,
        "/invoices/INV-1/record-payment",
        json!({"payment_amount": 40, "payment_date": "2026-11-10"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{part}");
    assert_eq!(part["paid_amount"], json!("40.000000"));
    assert_eq!(part["outstanding"], json!("60.000000"));
    assert_eq!(part["status"], json!("outstanding"));
    assert_eq!(part["journal_posted"], json!(false));

    // the rest settles it
    let (_, rest) = call(
        &router,
        Method::POST,
        "/invoices/INV-1/record-payment",
        json!({"payment_amount": 60}),
    )
    .await;
    assert_eq!(rest["status"], json!("paid"));
    assert_eq!(rest["outstanding"], json!("0.000000"));

    // both payments are kept, the invoice reads back, and an unknown number is missing
    let (_, kept) = call(&router, Method::GET, "/invoices/INV-1/payments", json!({})).await;
    assert_eq!(kept.as_array().map(Vec::len), Some(2));
    let (status, found) = call(&router, Method::GET, "/invoices/INV-1", json!({})).await;
    assert_eq!(status, StatusCode::OK, "{found}");
    assert_eq!(found["paid_amount"], json!("100.000000"));
    let (status, _) = call(&router, Method::GET, "/invoices/INV-9", json!({})).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // the customer reads back by its own identifier and a change touches only what it names
    let (_, changed) = call(
        &router,
        Method::PUT,
        "/customers/ACME-1",
        json!({"name": "Acme Ltd"}),
    )
    .await;
    assert_eq!(changed["name"], json!("Acme Ltd"));
    assert_eq!(changed["email"], json!("billing@acme.test"));

    // deleting twice answers that the second found nothing
    let (status, gone) = call(&router, Method::DELETE, "/invoices/INV-1", json!({})).await;
    assert_eq!(status, StatusCode::OK, "{gone}");
    assert_eq!(gone["ok"], json!(true));
    let (status, _) = call(&router, Method::DELETE, "/invoices/INV-1", json!({})).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
