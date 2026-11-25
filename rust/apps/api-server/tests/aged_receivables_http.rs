#![forbid(unsafe_code)]
//! Who owes the business, at the address the engine publishes, over a real Book.
//!
//! A person raises invoices and records the money that arrives. These tests open a Book the way a person does, raise two
//! invoices for one customer, record a receipt against one of them, then ask who is late and read back what is still
//! owed, how late it is, and the sentence they could send.

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

/// A Book with the accounts a sale and a receipt post to, two invoices for one customer, and one receipt.
async fn a_book_with_two_invoices_and_a_receipt() -> Arc<MemoryGraph> {
    let graph = Arc::new(MemoryGraph::new());
    for (number, name, kind, side) in [
        ("1000", "Cash", "asset", "debit"),
        ("1100", "Trade receivables", "asset", "debit"),
        ("4000", "Sales", "revenue", "credit"),
        ("4100", "Other income", "revenue", "credit"),
    ] {
        let (status, made) = call(
            graph.clone(),
            Method::POST,
            "/accounting/accounts",
            Some(
                json!({"account_number": number, "name": name, "account_type": kind,
                        "normal_balance": side}),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{made}");
    }
    for (number, date, due, total) in [
        ("INV-1", "2026-09-01", "2026-09-30", "300.00"),
        ("INV-2", "2026-10-01", "2026-10-31", "400.00"),
    ] {
        let (status, made) = call(
            graph.clone(),
            Method::POST,
            "/accounting/sales-journal",
            Some(json!({"invoice_number": number, "customer_id": "acme",
                        "entry_date": date, "due_date": due, "total_amount": total,
                        "tax_amount": "0", "status": "posted"})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{made}");
    }
    // the money that arrived names the newer invoice, so the older debt is still owed and still late
    let (status, made) = call(
        graph.clone(),
        Method::POST,
        "/accounting/cash-receipts-journal",
        Some(
            json!({"receipt_number": "RCT-1", "customer_id": "acme", "entry_date": "2026-11-02",
                    "amount": "400.00", "document_reference": "INV-2", "status": "posted"}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{made}");
    graph
}

#[tokio::test]
async fn what_is_still_owed_and_how_late_is_read_at_the_address() {
    let graph = a_book_with_two_invoices_and_a_receipt().await;
    let (status, answer) = call(
        graph,
        Method::GET,
        "/accounting/aged-receivables?as_of_date=2026-11-25",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{answer}");
    assert_eq!(answer["as_of"], json!("2026-11-25"));
    assert_eq!(answer["totals"]["owed"], json!("300.000000"));
    assert_eq!(answer["totals"]["late"], json!("300.000000"));
    // the invoice the money named is settled and gone from the answer; the older one is 56 days late
    let acme = &answer["customers"][0];
    assert_eq!(acme["customer_id"], json!("acme"));
    assert_eq!(acme["invoices"].as_array().map(Vec::len), Some(1));
    assert_eq!(acme["invoices"][0]["invoice_number"], json!("INV-1"));
    assert_eq!(acme["invoices"][0]["days_late"], json!(56));
    assert_eq!(acme["61_90"], json!("0.000000"));
    assert_eq!(acme["31_60"], json!("300.000000"));
    let reminder = acme["reminder"].as_str().expect("a sentence to send");
    assert!(reminder.contains("INV-1"), "{reminder}");
    assert!(reminder.contains("300.000000"), "{reminder}");
    assert!(reminder.contains("56 days late"), "{reminder}");
}

#[tokio::test]
async fn a_day_that_cannot_be_read_is_refused_in_words() {
    let graph = a_book_with_two_invoices_and_a_receipt().await;
    let (status, answer) = call(
        graph,
        Method::GET,
        "/accounting/aged-receivables?as_of_date=25%20November",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{answer}");
    let said = answer["error"].as_str().unwrap_or_default();
    assert!(said.contains("25 November"), "{said}");
    assert!(said.contains("YYYY-MM-DD"), "{said}");
}
