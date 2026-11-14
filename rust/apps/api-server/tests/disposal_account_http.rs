//! disposal-account-service: the profit or loss on a disposal, the journal it posts, and what the book holds afterwards.
//!
//! The service kept no storage, so its `/disposals` answered an empty list and its `/summary` answered zeros no matter
//! what had been posted. Posting now records the disposal, which is the point of these tests: the arithmetic is proven in
//! the crate, and here it is proven that the record survives the call and shows up in the reads.

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use serde_json::{json, Value};
use std::sync::Arc;
use store::fake::MemoryGraph;
use tower::ServiceExt;

struct Book {
    router: axum::Router,
}

impl Book {
    fn new() -> Self {
        Self {
            router: vimbai_api::records::router(Arc::new(MemoryGraph::new())),
        }
    }

    async fn call(&self, method: Method, uri: &str, payload: Value) -> (StatusCode, Value) {
        let request = Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json")
            .header("x-user-id", "user-1")
            .header("x-book-id", "book-1")
            .body(Body::from(payload.to_string()))
            .unwrap();
        let response = self.router.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 1_000_000)
            .await
            .unwrap();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }
}

fn sold_for_more_than_it_is_worth() -> Value {
    json!({
        "asset_id": "asset-1f2e3d4c",
        "asset_name": "Delivery van",
        "asset_code": "MV-1",
        "original_cost": "50000",
        "accumulated_depreciation": "30000",
        "disposal_proceeds": "25000",
        "disposal_date": "2026-10-31",
        "reason": "sold",
    })
}

/// An amount read as cents, so a figure is compared by value rather than by how many zeros it carries.
fn cents_of(raw: &str) -> i128 {
    let (whole, part) = raw.split_once('.').unwrap_or((raw, ""));
    let pad = format!("{:0<2}", &part[..part.len().min(2)]);
    whole.parse::<i128>().unwrap_or(0) * 100 + pad.parse::<i128>().unwrap_or(0)
}

/// A journal side in cents, so two sides of a double entry can be compared without a decimal crate in the test.
fn side_in_cents(journal: &Value, side: &str) -> i128 {
    journal["entries"]
        .as_array()
        .map(|lines| {
            lines
                .iter()
                .map(|line| {
                    let raw = line[side].as_str().unwrap_or("0");
                    let (whole, part) = raw.split_once('.').unwrap_or((raw, ""));
                    let pad = format!("{:0<2}", &part[..part.len().min(2)]);
                    whole.parse::<i128>().unwrap_or(0) * 100 + pad.parse::<i128>().unwrap_or(0)
                })
                .sum()
        })
        .unwrap_or(0)
}

#[tokio::test]
async fn a_disposal_is_worked_out_at_the_address_the_service_published() {
    let book = Book::new();
    let (status, entry) = book
        .call(
            Method::POST,
            "/disposal-account/disposal/calculate",
            sold_for_more_than_it_is_worth(),
        )
        .await;
    assert!(status.is_success(), "{status}: {entry}");
    // cost 50000 less 30000 worn is 20000 carried, sold for 25000, so 5000 of profit and no loss
    let amount = |field: &str| -> i128 { cents_of(entry[field].as_str().unwrap_or("0")) };
    assert_eq!(amount("net_book_value"), 2_000_000, "{entry}");
    assert_eq!(amount("profit_on_disposal"), 500_000, "{entry}");
    // no loss at all: the exact zero reads "0" rather than "0.00", which is the same figure
    assert_eq!(amount("loss_on_disposal"), 0, "{entry}");
    // and nothing was recorded by asking the question
    let (_, listed) = book
        .call(Method::GET, "/disposal-account/disposals", Value::Null)
        .await;
    assert_eq!(listed["count"], 0, "calculating must not post: {listed}");
}

#[tokio::test]
async fn posting_a_disposal_records_it_and_balances_its_journal() {
    let book = Book::new();
    let (status, posted) = book
        .call(
            Method::POST,
            "/disposal-account/disposal/post",
            sold_for_more_than_it_is_worth(),
        )
        .await;
    assert!(status.is_success(), "{status}: {posted}");
    assert_eq!(posted["status"], "posted", "{posted}");

    let journal = &posted["journal_entry"];
    assert_eq!(
        side_in_cents(journal, "debit"),
        side_in_cents(journal, "credit"),
        "the disposal journal does not balance: {journal}"
    );
    // cash 25000 in plus depreciation released 30000 against cost 50000 and profit 5000
    assert_eq!(
        side_in_cents(journal, "debit"),
        5_500_000,
        "the journal does not carry both sides: {journal}"
    );

    let (_, listed) = book
        .call(Method::GET, "/disposal-account/disposals", Value::Null)
        .await;
    assert_eq!(listed["count"], 1, "{listed}");
    assert_eq!(listed["disposals"][0]["asset_name"], "Delivery van");
}

#[tokio::test]
async fn a_narrowed_listing_reads_only_the_disposals_it_was_asked_for() {
    let book = Book::new();
    book.call(
        Method::POST,
        "/disposal-account/disposal/post",
        sold_for_more_than_it_is_worth(),
    )
    .await;
    let (status, posted) = book
        .call(
            Method::GET,
            "/disposal-account/disposals?status=posted",
            Value::Null,
        )
        .await;
    assert!(status.is_success(), "{status}: {posted}");
    assert_eq!(posted["count"], 1, "{posted}");
    let (_, drafted) = book
        .call(
            Method::GET,
            "/disposal-account/disposals?status=draft",
            Value::Null,
        )
        .await;
    assert_eq!(drafted["count"], 0, "{drafted}");
}

#[tokio::test]
async fn the_summary_adds_up_what_the_book_posted() {
    let book = Book::new();
    book.call(
        Method::POST,
        "/disposal-account/disposal/post",
        sold_for_more_than_it_is_worth(),
    )
    .await;
    // a second disposal at a loss: carried 8000, sold for 6000
    book.call(
        Method::POST,
        "/disposal-account/disposal/post",
        json!({
            "asset_id": "asset-2a3b4c5d",
            "asset_name": "Old press",
            "asset_code": "PE-9",
            "original_cost": "20000",
            "accumulated_depreciation": "12000",
            "disposal_proceeds": "6000",
            "disposal_date": "2026-11-02",
            "reason": "scrapped",
        }),
    )
    .await;

    let (status, summary) = book
        .call(Method::GET, "/disposal-account/summary", Value::Null)
        .await;
    assert!(status.is_success(), "{status}: {summary}");
    assert_eq!(summary["total_disposals"], 2, "{summary}");
    let amount = |field: &str| -> i128 { cents_of(summary[field].as_str().unwrap_or("0")) };
    assert_eq!(amount("total_proceeds"), 3_100_000, "{summary}");
    assert_eq!(amount("total_profit"), 500_000, "{summary}");
    assert_eq!(amount("total_loss"), 200_000, "{summary}");
    assert_eq!(amount("net_gain"), 300_000, "{summary}");
}

#[tokio::test]
async fn a_disposal_that_is_not_there_is_refused_not_recorded() {
    let book = Book::new();
    let (status, body) = book
        .call(
            Method::POST,
            "/disposal-account/disposal/post",
            json!({"asset_id": "asset-3", "original_cost": "lots"}),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    let (_, listed) = book
        .call(Method::GET, "/disposal-account/disposals", Value::Null)
        .await;
    assert_eq!(listed["count"], 0, "a refused post left a record: {listed}");
}
