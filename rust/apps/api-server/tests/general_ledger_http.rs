#![forbid(unsafe_code)]
//! The general ledger answers at the address the engine publishes, over a real Book.
//!
//! One account's ledger already answered at `/accounting/ledgers/{number}`, and the trial balance already listed every
//! account's closing figure. What these tests pin is the report in between, the one an accountant asks for by name: the
//! whole Book for a window, every account with what it opened on, what moved it, and what it closed on, in one answer.
//! The accounts and the entries are made through the same doors a person uses, so the report is read out of a Book that
//! was kept rather than out of rows arranged for it.

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

/// A Book with a chart and a journal, both made through the routes.
async fn a_kept_book() -> Arc<MemoryGraph> {
    let graph = Arc::new(MemoryGraph::new());
    for (number, name, kind, normal) in [
        ("1000", "Cash", "asset", "debit"),
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
    for (date, amount, status) in [
        ("2026-02-20", "100.00", "posted"),
        ("2026-03-05", "30.00", "posted"),
        // a draft is not in the book yet, so no report of what the Book holds may count it
        ("2026-03-20", "99.00", "draft"),
    ] {
        let (code, made) = call(
            graph.clone(),
            Method::POST,
            "/accounting/journal-entries",
            Some(json!({
                "entry_date": date,
                "description": "a sale",
                "status": status,
                "lines": [
                    {"account_number": "1000", "debit": amount, "credit": "0"},
                    {"account_number": "4000", "debit": "0", "credit": amount},
                ],
            })),
        )
        .await;
        assert_eq!(code, StatusCode::OK, "{made}");
    }
    graph
}

#[tokio::test]
async fn the_general_ledger_runs_the_whole_book_over_a_window() {
    let graph = a_kept_book().await;
    let (status, answer) = call(
        graph,
        Method::GET,
        "/accounting/general-ledger?start_date=2026-03-01&end_date=2026-03-31",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{answer}");

    let accounts = answer["accounts"].as_array().expect("accounts");
    assert_eq!(accounts.len(), 2, "{answer}");
    assert_eq!(answer["account_count"], json!(2));
    // the two posted lines of March, and nothing from the draft
    assert_eq!(answer["movement_count"], json!(2));

    // the asset opened on February's 100, was debited 30 in March, and closed on 130
    assert_eq!(accounts[0]["account_number"], json!("1000"));
    assert_eq!(accounts[0]["start_balance"], json!("100.000000"));
    assert_eq!(accounts[0]["total_debit"], json!("30.000000"));
    assert_eq!(accounts[0]["total_credit"], json!("0.000000"));
    assert_eq!(accounts[0]["end_balance"], json!("130.000000"));
    assert_eq!(
        accounts[0]["entries"].as_array().expect("rows").len(),
        1,
        "{answer}"
    );
    assert_eq!(accounts[0]["entries"][0]["balance"], json!("130.000000"));

    // the revenue opened on February's 100 too, credited 30 in March, on its own side
    assert_eq!(accounts[1]["account_number"], json!("4000"));
    assert_eq!(accounts[1]["start_balance"], json!("100.000000"));
    assert_eq!(accounts[1]["total_credit"], json!("30.000000"));
    assert_eq!(accounts[1]["end_balance"], json!("130.000000"));

    // and the proof: what the window debited is what it credited
    assert_eq!(answer["total_debit"], json!("30.000000"));
    assert_eq!(answer["total_credit"], json!("30.000000"));
    assert_eq!(answer["is_balanced"], json!(true));
}

#[tokio::test]
async fn the_general_ledger_leaves_out_a_draft_and_says_where_the_window_ends() {
    let graph = a_kept_book().await;
    // no window at all: the report runs from the first entry the Book holds to the last
    let (status, answer) = call(graph, Method::GET, "/accounting/general-ledger", None).await;
    assert_eq!(status, StatusCode::OK, "{answer}");
    assert_eq!(answer["movement_count"], json!(4), "{answer}");
    assert_eq!(answer["accounts"][0]["end_balance"], json!("130.000000"));
    assert_eq!(answer["start_date"], json!(""));
    assert_eq!(answer["end_date"], json!(""));
}

#[tokio::test]
async fn the_general_ledger_narrows_to_one_account_on_request() {
    let graph = a_kept_book().await;
    let (status, answer) = call(
        graph,
        Method::GET,
        "/accounting/general-ledger?account_number=4000",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{answer}");
    assert_eq!(answer["account_count"], json!(1));
    assert_eq!(answer["accounts"][0]["account_number"], json!("4000"));
    assert_eq!(answer["accounts"][0]["account_type"], json!("revenue"));
}

#[tokio::test]
async fn the_general_ledger_refuses_an_account_the_book_does_not_hold() {
    let graph = a_kept_book().await;
    let (status, refused) = call(
        graph,
        Method::GET,
        "/accounting/general-ledger?account_number=9999",
        None,
    )
    .await;
    // an empty report would read as an account that did nothing, which is a different statement from "no such account"
    assert_eq!(status, StatusCode::NOT_FOUND, "{refused}");
}

#[tokio::test]
async fn one_account_reads_the_same_through_both_ledger_doors() {
    let graph = a_kept_book().await;
    let window = "start_date=2026-03-01&end_date=2026-03-31";
    let (status, alone) = call(
        graph.clone(),
        Method::GET,
        &format!("/accounting/ledgers/1000?{window}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{alone}");
    let (status, among) = call(
        graph,
        Method::GET,
        &format!("/accounting/general-ledger?{window}&account_number=1000"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{among}");
    // one body serves both, so the same Book and the same window cannot answer two ways
    assert_eq!(alone, among["accounts"][0]);
}
