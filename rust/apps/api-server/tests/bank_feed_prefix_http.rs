#![forbid(unsafe_code)]
//! bank-feed-service answers at the addresses it published.
//!
//! The rules already lived in the banking crate and answered at bare paths - `/transactions`, `/rules`, `/connections` -
//! which other services publish too. So the prefix is part of the address here, and this pins that the addresses a client
//! calls are the ones that answer: creating a connection, reading it back, deleting it, a rule that classifies an imported
//! transaction, and the statistics over both.

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

#[tokio::test]
async fn a_connection_is_created_listed_and_deleted_at_the_prefix() {
    let graph = Arc::new(MemoryGraph::new());
    let (status, created) = call(
        graph.clone(),
        Method::POST,
        "/bank-feed/connections?organization_id=org-1",
        Some(json!({
            "provider": "plaid",
            "account_name": "Operating",
            "account_number_last4": "4242",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "creating a connection: {created}");
    let id = created["id"]
        .as_str()
        .or_else(|| created["connection"]["id"].as_str())
        .unwrap_or_else(|| panic!("no connection id in {created}"))
        .to_string();

    let (status, listed) = call(
        graph.clone(),
        Method::GET,
        "/bank-feed/connections?organization_id=org-1",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "listing connections: {listed}");
    assert!(
        listed.to_string().contains(&id),
        "the connection just created is not in the listing: {listed}"
    );

    let (status, _) = call(
        graph.clone(),
        Method::DELETE,
        &format!("/bank-feed/connections/{id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "deleting the connection");
}

#[tokio::test]
async fn a_rule_is_created_and_listed_at_the_prefix() {
    let graph = Arc::new(MemoryGraph::new());
    let (status, rule) = call(
        graph.clone(),
        Method::POST,
        "/bank-feed/rules?organization_id=org-1",
        Some(json!({
            "name": "Coffee",
            "description": "anything from the coffee shop",
            "match_conditions": {"description": {"contains": "COFFEE"}},
            "priority": 1,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "creating a rule: {rule}");
    let (status, listed) = call(
        graph,
        Method::GET,
        "/bank-feed/rules?organization_id=org-1",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "listing rules: {listed}");
    assert!(
        listed.to_string().contains("Coffee"),
        "the rule just created is not in the listing: {listed}"
    );
    assert!(
        listed["total"].as_u64().unwrap_or_default() >= 1,
        "the listing reports no rules: {listed}"
    );
}

#[tokio::test]
async fn the_root_and_the_statistics_answer_at_the_prefix() {
    let graph = Arc::new(MemoryGraph::new());
    let (status, health) = call(graph.clone(), Method::GET, "/bank-feed/", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(health["service"], "bank-feed-service");

    let (status, figures) = call(
        graph,
        Method::GET,
        "/bank-feed/statistics?organization_id=org-1",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "statistics: {figures}");
}

#[tokio::test]
async fn a_transaction_is_listed_for_its_connection_and_reconciled_against_an_entry_in_one_call() {
    let graph = Arc::new(MemoryGraph::new());
    let (status, created) = call(
        graph.clone(),
        Method::POST,
        "/bank-feed/connections?organization_id=org-1",
        Some(json!({
            "provider": "plaid",
            "account_name": "Operating",
            "account_number_last4": "4242",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "creating a connection: {created}");
    let connection = created["id"]
        .as_str()
        .or_else(|| created["connection"]["id"].as_str())
        .unwrap_or_else(|| panic!("no connection id in {created}"))
        .to_string();

    let (status, imported) = call(
        graph.clone(),
        Method::POST,
        "/bank-feed/transactions/import",
        Some(json!({
            "bank_connection_id": connection,
            "external_id": "bank-line-1",
            "date": "2026-11-15",
            "amount": "250.00",
            "description": "CLIENT PAYMENT",
            "transaction_type": "credit",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "importing: {imported}");
    let transaction = imported["id"]
        .as_str()
        .unwrap_or_else(|| panic!("no transaction id in {imported}"))
        .to_string();

    // the screen lists the feed for the connection it is looking at, not the whole Book
    let (status, listed) = call(
        graph.clone(),
        Method::GET,
        &format!("/bank-feed/transactions?connection_id={connection}&limit=10&offset=0"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "listing the feed: {listed}");
    assert!(
        listed.to_string().contains(&transaction),
        "the transaction just imported is not in the feed for its connection: {listed}"
    );

    // and reconciling is one act: the status and the entry it matched, not two calls that can disagree
    let (status, reconciled) = call(
        graph.clone(),
        Method::POST,
        &format!("/bank-feed/transactions/{transaction}/reconcile"),
        Some(json!({ "matched_entry_id": "entry-7", "notes": "spoke to the client" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "reconciling: {reconciled}");
    assert_eq!(reconciled["status"], "reconciled", "in {reconciled}");
    assert_eq!(
        reconciled["journal_entry_id"], "entry-7",
        "the entry it was reconciled against: {reconciled}"
    );

    let (status, read_back) = call(
        graph,
        Method::GET,
        &format!("/bank-feed/transactions/{transaction}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "reading it back: {read_back}");
    assert_eq!(read_back["status"], "reconciled");
    assert_eq!(read_back["journal_entry_id"], "entry-7");
}
