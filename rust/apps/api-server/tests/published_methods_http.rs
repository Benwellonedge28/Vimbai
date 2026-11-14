#![forbid(unsafe_code)]
//! The last three published methods that reached nothing, now answered.
//!
//! Two were banking-integration's own connection routes: the connection's transaction feed and its sweep. The third
//! was personal finance's in-place recurring update, which the service allowed and the engine only partially covered
//! (open, run and stop, but nothing that changed the amount). Each test walks the address the gateway forwards and
//! checks the answer against what the service did, so a method gap cannot reopen silently.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use serde_json::{json, Value};
use store::fake::MemoryGraph;
use tower::ServiceExt;

/// An amount as a number, so "500" and "500.000000" are the same figure.
fn figure(value: &Value) -> f64 {
    value
        .as_str()
        .and_then(|text| text.parse::<f64>().ok())
        .or_else(|| value.as_f64())
        .unwrap_or_default()
}

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
    let bytes = axum::body::to_bytes(response.into_body(), 1_000_000)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

// ---------------------------------------------------------------- personal finance: the recurring update

async fn a_recurring_item(graph: &Arc<MemoryGraph>) -> String {
    let (status, opened) = call(
        Arc::clone(graph),
        Method::POST,
        "/personal-finance/recurring",
        Some(json!({
            "kind": "bill",
            "description": "Rent",
            "amount": "450",
            "currency": "USD",
            "frequency": "monthly",
            "next_due": "2026-12-01",
            "auto_record": false,
        })),
    )
    .await;
    assert!(status.is_success(), "{status}: {opened}");
    opened["id"].as_str().unwrap_or_default().to_string()
}

#[tokio::test]
async fn a_rent_increase_updates_the_same_recurring_item() {
    let graph = Arc::new(MemoryGraph::new());
    let item = a_recurring_item(&graph).await;

    let (status, updated) = call(
        Arc::clone(&graph),
        Method::PATCH,
        &format!("/personal-finance/recurring/{item}"),
        Some(json!({"amount": "500"})),
    )
    .await;
    assert!(status.is_success(), "{status}: {updated}");
    assert_eq!(figure(&updated["amount"]), 500.0, "{updated}");
    assert_eq!(updated["description"], "Rent", "{updated}");
    assert_eq!(updated["frequency"], "monthly", "{updated}");

    // and the list agrees: one rent, now dearer, rather than a second item
    let (_, listed) = call(
        Arc::clone(&graph),
        Method::GET,
        "/personal-finance/recurring",
        None,
    )
    .await;
    let rows = listed["items"]
        .as_array()
        .or_else(|| listed.as_array())
        .expect("a list of recurring items");
    assert_eq!(rows.len(), 1, "{listed}");
    assert_eq!(figure(&rows[0]["amount"]), 500.0, "{listed}");
}

#[tokio::test]
async fn a_recurring_update_moves_the_next_due_date_and_the_frequency() {
    let graph = Arc::new(MemoryGraph::new());
    let item = a_recurring_item(&graph).await;

    let (status, updated) = call(
        Arc::clone(&graph),
        Method::PATCH,
        &format!("/personal-finance/recurring/{item}"),
        Some(json!({"frequency": "quarterly", "next_due": "2027-01-01"})),
    )
    .await;
    assert!(status.is_success(), "{status}: {updated}");
    assert_eq!(updated["frequency"], "quarterly", "{updated}");
    assert_eq!(updated["next_due"], "2027-01-01", "{updated}");
    assert_eq!(
        figure(&updated["amount"]),
        450.0,
        "the amount was not touched: {updated}"
    );
}

#[tokio::test]
async fn a_recurring_update_refuses_an_unknown_frequency_and_an_empty_change() {
    let graph = Arc::new(MemoryGraph::new());
    let item = a_recurring_item(&graph).await;

    let (status, refused) = call(
        Arc::clone(&graph),
        Method::PATCH,
        &format!("/personal-finance/recurring/{item}"),
        Some(json!({"frequency": "fortnightly"})),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{refused}");
    assert!(
        refused["error"]
            .as_str()
            .unwrap_or_default()
            .contains("frequency"),
        "{refused}"
    );

    let (status, refused) = call(
        Arc::clone(&graph),
        Method::PATCH,
        &format!("/personal-finance/recurring/{item}"),
        Some(json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{refused}");
    assert!(
        refused["error"]
            .as_str()
            .unwrap_or_default()
            .contains("nothing to update"),
        "{refused}"
    );
}

#[tokio::test]
async fn updating_a_recurring_item_that_does_not_exist_is_not_found() {
    let graph = Arc::new(MemoryGraph::new());
    let (status, _) = call(
        graph,
        Method::PATCH,
        "/personal-finance/recurring/r-missing",
        Some(json!({"amount": "10"})),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ---------------------------------------------------------------- banking integration: the connection's own routes

async fn a_connection(graph: &Arc<MemoryGraph>) -> String {
    let (status, opened) = call(
        Arc::clone(graph),
        Method::POST,
        "/connect",
        Some(json!({
            "bank_name": "CBZ",
            "account_number": "0123456789",
            "account_type": "checking",
            "today": "2026-11-14",
        })),
    )
    .await;
    assert!(status.is_success(), "{status}: {opened}");
    opened["id"].as_str().unwrap_or_default().to_string()
}

#[tokio::test]
async fn a_connection_is_swept_at_its_own_published_address() {
    let graph = Arc::new(MemoryGraph::new());
    let connection = a_connection(&graph).await;

    // banking-integration publishes /sync/{connection_id} on a stripped prefix, so the engine answers it flat
    let (status, swept) = call(
        Arc::clone(&graph),
        Method::POST,
        &format!("/sync/{connection}"),
        Some(json!({"today": "2026-11-14"})),
    )
    .await;
    assert!(status.is_success(), "{status}: {swept}");
    assert_eq!(swept["connection_id"], connection.as_str(), "{swept}");
    assert_eq!(swept["synced_at"], "2026-11-14", "{swept}");
    assert_eq!(swept["status"], "success", "{swept}");

    // the sweep is recorded on the connection, so the next caller can see when it last ran
    let (_, connections) = call(Arc::clone(&graph), Method::GET, "/connections", None).await;
    let rows = connections
        .as_array()
        .or_else(|| connections["connections"].as_array())
        .expect("the connections the book holds");
    let held = rows
        .iter()
        .find(|row| row["id"] == connection.as_str())
        .expect("the connection he opened");
    assert_eq!(held["last_sync"], "2026-11-14", "{connections}");
}

#[tokio::test]
async fn the_transaction_feed_of_one_connection_answers_at_its_published_address() {
    let graph = Arc::new(MemoryGraph::new());
    let connection = a_connection(&graph).await;

    let (status, feed) = call(
        Arc::clone(&graph),
        Method::GET,
        &format!("/transactions/{connection}?limit=5"),
        None,
    )
    .await;
    // nothing imported over this connection yet, so the feed is empty rather than an error
    assert!(status.is_success(), "{status}: {feed}");
    assert!(feed.is_array(), "{feed}");
    assert!(feed.as_array().unwrap().is_empty(), "{feed}");
}

#[tokio::test]
async fn a_feed_for_a_connection_the_book_does_not_hold_is_not_found() {
    let graph = Arc::new(MemoryGraph::new());
    let (status, _) = call(graph, Method::GET, "/transactions/conn-missing", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn sweeping_a_connection_the_book_no_longer_holds_is_not_found() {
    let graph = Arc::new(MemoryGraph::new());
    let connection = a_connection(&graph).await;

    let (status, closed) = call(
        Arc::clone(&graph),
        Method::DELETE,
        &format!("/connections/{connection}"),
        None,
    )
    .await;
    assert!(status.is_success(), "{status}: {closed}");

    let (status, refused) = call(
        Arc::clone(&graph),
        Method::POST,
        &format!("/sync/{connection}"),
        Some(json!({"today": "2026-11-14"})),
    )
    .await;
    // the flat /connections/{id} delete removes the connection, so a sweep of it finds nothing to sweep.
    // The refusal for a connection that is merely closed is covered where it lives, in the banking crate.
    assert_eq!(status, StatusCode::NOT_FOUND, "{refused}");
}
