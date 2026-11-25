#![forbid(unsafe_code)]
//! What the business owes, at the address the engine publishes, over a real Book.
//!
//! The mirror of the receivables test. A person receives two bills from one supplier and records the money that goes
//! out, then asks what is late and reads back what is still owed, how late it is, and the sentence saying what to pay.

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

/// A Book with the accounts a bill and a payment post to, two bills from one supplier, and one payment.
async fn a_book_with_two_bills_and_a_payment() -> Arc<MemoryGraph> {
    let graph = Arc::new(MemoryGraph::new());
    for (number, name, kind, side) in [
        ("1010", "Bank", "asset", "debit"),
        ("2000", "Accounts payable", "liability", "credit"),
        ("6000", "Costs", "expense", "debit"),
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
        ("B-1", "2026-09-01", "2026-09-30", "300.00"),
        ("B-2", "2026-10-01", "2026-10-31", "400.00"),
    ] {
        let (status, made) = call(
            graph.clone(),
            Method::POST,
            "/accounting/vendor-bills",
            Some(json!({"bill_number": number, "vendor_id": "steel",
                        "vendor_name": "Steel Traders", "entry_date": date, "due_date": due,
                        "total_amount": total, "status": "pending"})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{made}");
    }
    let (status, money) = call(
        graph.clone(),
        Method::POST,
        "/cashbook/accounts",
        Some(
            json!({"account_code": "BANK-1", "account_name": "Bank", "account_type": "bank",
                    "opening_balance": "0.00", "currency": "USD"}),
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "the Book registers its money: {money}"
    );

    // the money that left names the newer bill, so the older debt is still owed and still late
    let (status, made) = call(
        graph.clone(),
        Method::POST,
        "/accounting/cash-disbursements-journal",
        Some(
            json!({"payment_number": "PAY-1", "vendor_id": "steel", "entry_date": "2026-11-02",
                    "amount": "400.00", "document_reference": "B-2", "status": "posted"}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{made}");
    graph
}

#[tokio::test]
async fn what_is_still_owed_to_a_supplier_and_how_late_is_read_at_the_address() {
    let graph = a_book_with_two_bills_and_a_payment().await;
    let (status, answer) = call(
        graph,
        Method::GET,
        "/accounting/aged-payables?as_of_date=2026-11-25",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{answer}");
    assert_eq!(answer["as_of"], json!("2026-11-25"));
    assert_eq!(answer["totals"]["owed"], json!("300.000000"));
    assert_eq!(answer["totals"]["late"], json!("300.000000"));
    // the bill the money named is settled and gone from the answer; the older one is 56 days late
    let steel = &answer["vendors"][0];
    assert_eq!(steel["vendor_id"], json!("steel"));
    assert_eq!(steel["bills"].as_array().map(Vec::len), Some(1));
    assert_eq!(steel["bills"][0]["bill_number"], json!("B-1"));
    assert_eq!(steel["bills"][0]["days_late"], json!(56));
    assert_eq!(steel["bands"]["31_60"], json!("300.000000"));
    let sentence = steel["reminder"]
        .as_str()
        .expect("a sentence saying what to pay");
    assert!(sentence.contains("B-1"), "{sentence}");
    assert!(sentence.contains("300.00"), "{sentence}");
    assert!(sentence.contains("56 days late"), "{sentence}");
}

#[tokio::test]
async fn a_day_that_cannot_be_read_is_refused_in_words_on_the_payables_side_too() {
    let graph = a_book_with_two_bills_and_a_payment().await;
    let (status, answer) = call(
        graph,
        Method::GET,
        "/accounting/aged-payables?as_of_date=25%20November",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{answer}");
}
