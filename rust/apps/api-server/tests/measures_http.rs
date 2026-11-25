#![forbid(unsafe_code)]
//! What a Book measures itself by, at the address the engine publishes, over a real Book.
//!
//! The figures a business watches are rows in the Book rather than a list in the engine, so these tests open a Book the
//! way a person does, read the measures it starts with, add one of their own, and take one away. Everything is read out
//! of a Book that was kept, through the same doors a phone uses.

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

/// A Book with a chart, one taxed sale and one bill, posted the way the journals post them.
async fn a_book_that_traded() -> Arc<MemoryGraph> {
    let graph = Arc::new(MemoryGraph::new());
    for (number, name, kind, normal) in [
        ("1000", "Cash", "asset", "debit"),
        ("1100", "Accounts Receivable", "asset", "debit"),
        ("2000", "Accounts Payable", "liability", "credit"),
        ("2100", "VAT Payable", "liability", "credit"),
        ("4000", "Sales", "revenue", "credit"),
        ("6100", "Rent", "expense", "debit"),
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
    for (date, reference, lines) in [
        (
            "2026-10-05",
            "INV-1",
            json!([
                {"account_number": "1100", "debit": "1150.00", "credit": "0"},
                {"account_number": "4000", "debit": "0", "credit": "1000.00"},
                {"account_number": "2100", "debit": "0", "credit": "150.00"},
            ]),
        ),
        (
            "2026-10-09",
            "BILL-1",
            json!([
                {"account_number": "6100", "debit": "400.00", "credit": "0"},
                {"account_number": "2100", "debit": "60.00", "credit": "0"},
                {"account_number": "2000", "debit": "0", "credit": "460.00"},
            ]),
        ),
    ] {
        let (status, made) = call(
            graph.clone(),
            Method::POST,
            "/accounting/journal-entries",
            Some(json!({
                "entry_date": date,
                "description": reference,
                "reference_number": reference,
                "source_module": "Invoicing",
                "status": "posted",
                "lines": lines,
            })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{made}");
    }
    // the figures a new Book is opened with, asked for the way a Book asks for its standard rules
    let (status, installed) = call(
        graph.clone(),
        Method::POST,
        "/accounting/measures/standard",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{installed}");
    assert_eq!(
        installed["added"].as_array().map(Vec::len),
        Some(9),
        "{installed}"
    );

    // and one of the person's own, as a row
    let (status, opened) = call(
        graph.clone(),
        Method::POST,
        "/accounting/measures",
        Some(
            json!({"name": "premises", "label": "Premises", "source": "accounts",
                    "numbers": ["6100"]}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{opened}");
    graph
}

fn measure<'a>(answer: &'a Value, name: &str) -> &'a Value {
    answer["measures"]
        .as_array()
        .expect("measures")
        .iter()
        .find(|row| row["name"] == json!(name))
        .unwrap_or_else(|| panic!("no measure {name} in {answer}"))
}

#[tokio::test]
async fn a_book_answers_the_figures_it_measures_itself_by() {
    let graph = a_book_that_traded().await;
    let (status, answer) = call(
        graph,
        Method::GET,
        "/accounting/measures?from=2026-10-01&to=2026-10-31",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{answer}");
    assert_eq!(answer["from"], json!("2026-10-01"));
    assert_eq!(answer["to"], json!("2026-10-31"));
    // what the accounts took up, worked out from the Book's own revenue and expense accounts
    assert_eq!(measure(&answer, "revenue")["value"], json!("1000.000000"));
    assert_eq!(measure(&answer, "costs")["value"], json!("400.000000"));
    assert_eq!(measure(&answer, "profit")["value"], json!("600.000000"));
    // what is owed each way reads the Book's documents, and this Book posted journal entries without raising one, so both
    // answer nothing rather than counting the entries as debts
    assert_eq!(measure(&answer, "owed_to_you")["value"], json!("0.000000"));
    assert_eq!(measure(&answer, "owed_by_you")["value"], json!("0.000000"));
    // a measure added as a row counts the accounts it names
    assert_eq!(measure(&answer, "premises")["value"], json!("400.000000"));
}

#[tokio::test]
async fn a_measure_a_person_adds_is_a_row_that_can_be_taken_away() {
    let graph = a_book_that_traded().await;
    let (status, made) = call(
        graph.clone(),
        Method::POST,
        "/accounting/measures",
        Some(
            json!({"name": "share_of_trading", "label": "Share of trading", "source": "percent",
                    "of": "profit", "over": "revenue", "unit": "percent"}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{made}");

    let (_, answer) = call(
        graph.clone(),
        Method::GET,
        "/accounting/measures?to=2026-10-31",
        None,
    )
    .await;
    // the month so far is the window a person standing in a month means
    assert_eq!(answer["from"], json!("2026-10-01"));
    assert_eq!(
        measure(&answer, "share_of_trading")["value"],
        json!("60.000000")
    );

    let (status, gone) = call(
        graph.clone(),
        Method::DELETE,
        "/accounting/measures/share_of_trading",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{gone}");
    let (_, answer) = call(
        graph.clone(),
        Method::GET,
        "/accounting/measures?to=2026-10-31",
        None,
    )
    .await;
    assert!(!answer["measures"]
        .as_array()
        .expect("measures")
        .iter()
        .any(|row| row["name"] == json!("share_of_trading")));
    let (status, refused) = call(
        graph,
        Method::DELETE,
        "/accounting/measures/share_of_trading",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{refused}");
}

#[tokio::test]
async fn a_measure_naming_an_account_this_book_does_not_keep_is_refused_in_words() {
    let graph = a_book_that_traded().await;
    let (status, refused) = call(
        graph,
        Method::POST,
        "/accounting/measures",
        Some(json!({"name": "stables", "label": "Stables", "source": "accounts", "types": ["livestock"]})),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{refused}");
    let said = refused["error"].as_str().unwrap_or_default().to_string();
    assert!(
        said.contains("keeps no account of type livestock"),
        "{said}"
    );
    assert!(said.contains("expense"), "{said}");
}

#[tokio::test]
async fn reading_the_measures_without_a_day_is_refused_rather_than_assuming_today() {
    let graph = a_book_that_traded().await;
    let (status, refused) = call(graph, Method::GET, "/accounting/measures", None).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{refused}");
    assert!(
        refused["error"]
            .as_str()
            .unwrap_or_default()
            .contains("as to=2026-10-20"),
        "{refused}"
    );
}
