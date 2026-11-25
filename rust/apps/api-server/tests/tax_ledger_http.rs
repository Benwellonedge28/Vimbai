#![forbid(unsafe_code)]
//! The tax ledger answers at the address the engine publishes, over a real Book.
//!
//! `/accounting/tax/` already answered what was collected, what could be reclaimed and what was owed over a period. What
//! these tests pin is the account behind that figure, and the posting half the Book did not have: paying the tax over,
//! so the liability leaves the books with the money instead of standing for ever. The accounts, the taxed sale and the
//! payment are made through the same doors a person uses, so both answers are read out of a Book that was kept.

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

/// A Book with a chart that holds a tax account, and one sale that charged tax on it.
async fn a_book_that_charged_tax() -> Arc<MemoryGraph> {
    let graph = Arc::new(MemoryGraph::new());
    for (number, name, kind, normal) in [
        ("1000", "Cash", "asset", "debit"),
        ("1100", "Accounts Receivable", "asset", "debit"),
        ("2100", "VAT Payable", "liability", "credit"),
        ("4000", "Sales", "revenue", "credit"),
    ] {
        let (status, made) = call(
            graph.clone(),
            Method::POST,
            "/accounting/accounts",
            Some(json!({
                "account_number": number,
                "name": name,
                "account_type": kind,
                "normal_balance": normal,
            })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{made}");
    }
    // a sale of 1,150 that carried 150 of tax, posted the way the sales journal posts one
    let (status, made) = call(
        graph.clone(),
        Method::POST,
        "/accounting/journal-entries",
        Some(json!({
            "entry_date": "2026-10-05",
            "description": "Sale INV-1",
            "reference_number": "INV-1",
            "source_module": "Invoicing",
            "status": "posted",
            "lines": [
                {"account_number": "1100", "debit": "1150.00", "credit": "0"},
                {"account_number": "4000", "debit": "0", "credit": "1000.00"},
                {"account_number": "2100", "debit": "0", "credit": "150.00"},
            ],
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{made}");
    graph
}

#[tokio::test]
async fn the_tax_ledger_states_each_tax_account_and_what_it_owes() {
    let graph = a_book_that_charged_tax().await;
    let (status, answer) = call(
        graph,
        Method::GET,
        "/accounting/tax-ledger?from=2026-10-01&to=2026-10-31",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{answer}");
    assert_eq!(answer["has_tax_account"], json!(true));

    let taxes = answer["taxes"].as_array().expect("taxes");
    let vat = taxes
        .iter()
        .find(|row| row["account_number"] == json!("2100"))
        .expect("the VAT account");
    assert_eq!(vat["charged"], json!("150.000000"));
    assert_eq!(vat["closing"], json!("150.000000"));
    assert_eq!(
        vat["movements"].as_array().expect("movements").len(),
        1,
        "{answer}"
    );
    assert_eq!(vat["movements"][0]["reference"], json!("INV-1"));
    assert_eq!(answer["totals"]["closing"], json!("150.000000"));
}

#[tokio::test]
async fn paying_the_tax_over_is_posted_and_leaves_nothing_owed() {
    let graph = a_book_that_charged_tax().await;
    let (status, paid) = call(
        graph.clone(),
        Method::POST,
        "/accounting/tax-ledger/settle",
        Some(json!({"entry_date": "2026-11-05", "amount": "150.00", "period": "2026-10"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{paid}");
    assert_eq!(paid["tax_account_number"], json!("2100"));
    assert_eq!(paid["money_account_number"], json!("1000"));
    assert_eq!(paid["owed_after"], json!("0.000000"));

    // the account the tax was owed in now holds nothing, and the payment is the ledger's own entry
    let (_, answer) = call(
        graph,
        Method::GET,
        "/accounting/tax-ledger?from=2026-10-01&to=2026-11-30",
        None,
    )
    .await;
    let vat = answer["taxes"]
        .as_array()
        .expect("taxes")
        .iter()
        .find(|row| row["account_number"] == json!("2100"))
        .expect("the VAT account");
    assert_eq!(vat["paid"], json!("150.000000"));
    assert_eq!(vat["closing"], json!("0.000000"));
}

#[tokio::test]
async fn a_payment_of_more_than_is_owed_is_refused_in_words() {
    let graph = a_book_that_charged_tax().await;
    let (status, refused) = call(
        graph,
        Method::POST,
        "/accounting/tax-ledger/settle",
        Some(json!({"entry_date": "2026-11-05", "amount": "200.00"})),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{refused}");
    let said = refused.to_string();
    assert!(said.contains("more than the 150.00 owed"), "{said}");
}

#[tokio::test]
async fn a_book_with_no_tax_account_says_so_rather_than_answering_nothing_owed() {
    let graph = Arc::new(MemoryGraph::new());
    let (status, answer) = call(
        graph,
        Method::GET,
        "/accounting/tax-ledger?from=2026-10-01&to=2026-10-31",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{answer}");
    assert_eq!(answer["has_tax_account"], json!(false));
    assert_eq!(answer["taxes"], json!([]));
}
