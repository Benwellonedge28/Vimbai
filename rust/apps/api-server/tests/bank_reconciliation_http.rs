#![forbid(unsafe_code)]
//! A bank statement matched against the cash book, at the address the engine publishes, over a real Book.
//!
//! A person records what they did - a receipt, a payment, a cheque written - and the statement arrives later from the
//! bank. These tests open a Book the way a person does, write the entries into its cash book, then hand the engine a
//! statement and read back what matches, what each side holds alone, and the proof that the two balances differ by
//! exactly those lines.

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

/// A Book with a bank account opened at 1,000.00 and three posted entries: a receipt, a payment, and a cheque the bank
/// has not seen yet.
async fn a_book_with_a_bank_account() -> Arc<MemoryGraph> {
    let graph = Arc::new(MemoryGraph::new());
    let (status, made) = call(
        graph.clone(),
        Method::POST,
        "/cashbook/accounts",
        Some(json!({"account_code": "1020", "account_name": "Current account", "opening_balance": "1000.00"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{made}");
    for (date, amount, debit, reference) in [
        ("2026-10-05", "500.00", true, "INV-1"),
        ("2026-10-09", "200.00", false, "BILL-1"),
        ("2026-10-28", "75.00", false, "CHQ-9"),
    ] {
        let (status, made) = call(
            graph.clone(),
            Method::POST,
            "/cashbook/entries",
            Some(json!({
                "bank_account": "1020",
                "entry_date": date,
                "amount": amount,
                "is_debit": debit,
                "reference": reference,
                "description": format!("entry {reference}"),
                "voucher_number": reference,
                "status": "posted",
            })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{made}");
    }
    graph
}

#[tokio::test]
async fn a_statement_is_matched_and_the_difference_is_proved() {
    let graph = a_book_with_a_bank_account().await;
    let (status, answer) = call(
        graph,
        Method::POST,
        "/accounting/bank-reconciliation/match",
        Some(json!({
            "bank_account": "1020",
            "statement_balance": "1287.50",
            "lines": [
                {"date": "2026-10-05", "amount": "500.00", "side": "in", "reference": "INV-1"},
                {"date": "2026-10-09", "amount": "200.00", "side": "out", "reference": "BILL-1"},
                {"date": "2026-10-31", "amount": "12.50", "side": "out", "description": "monthly charge"}
            ]
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{answer}");
    // the receipt and the payment are the same movement on both sides; the charge is the bank's alone and the cheque
    // the books' alone
    assert_eq!(answer["matched"].as_array().map(Vec::len), Some(2));
    assert_eq!(
        answer["in_statement_only"].as_array().map(Vec::len),
        Some(1)
    );
    assert_eq!(
        answer["in_statement_only"][0]["description"],
        json!("monthly charge")
    );
    assert_eq!(answer["in_books_only"].as_array().map(Vec::len), Some(1));
    assert_eq!(answer["in_books_only"][0]["reference"], json!("CHQ-9"));
    // and the two balances differ by exactly those two lines, so nothing is left unexplained
    assert_eq!(answer["book_balance"], json!("1225.000000"));
    assert_eq!(answer["difference"], json!("62.500000"));
    assert_eq!(answer["unexplained_difference"], json!("0.000000"));
    assert_eq!(answer["agrees"], json!(true));
}

#[tokio::test]
async fn an_account_the_book_does_not_keep_is_refused_in_words() {
    let graph = a_book_with_a_bank_account().await;
    let (status, answer) = call(
        graph,
        Method::POST,
        "/accounting/bank-reconciliation/match",
        Some(json!({"bank_account": "9999", "statement_balance": "0.00", "lines": []})),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{answer}");
    assert!(
        answer
            .to_string()
            .contains("no cash account under the code `9999`"),
        "{answer}"
    );
}

#[tokio::test]
async fn a_line_that_does_not_say_which_way_the_money_went_is_refused() {
    let graph = a_book_with_a_bank_account().await;
    let (status, answer) = call(
        graph,
        Method::POST,
        "/accounting/bank-reconciliation/match",
        Some(json!({
            "bank_account": "1020",
            "statement_balance": "1000.00",
            "lines": [{"date": "2026-10-05", "amount": "500.00"}]
        })),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{answer}");
    assert!(answer.to_string().contains("`in` or `out`"), "{answer}");
}
