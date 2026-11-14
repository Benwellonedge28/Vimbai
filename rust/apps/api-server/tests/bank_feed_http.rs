//! bank-feed-service answers at the prefix it publishes.
//!
//! The imports themselves always lived in the banking crate; what these tests pin is that the addresses the service
//! published answer, and that they answer with the same handler as the engine's own: a single transaction, a batch, an
//! MT940 statement, and the rule application that follows them.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use serde_json::{json, Value};
use store::fake::MemoryGraph;
use tower::ServiceExt;

/// The amount as a caller reads it, with the storage scale's trailing zeros trimmed.
fn amount_of(body: &Value) -> String {
    let raw = body["amount"].as_str().unwrap_or_default().to_owned();
    if raw.contains('.') {
        raw.trim_end_matches('0').trim_end_matches('.').to_owned()
    } else {
        raw
    }
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
    let bytes = axum::body::to_bytes(response.into_body(), 2_000_000)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

/// A connection to import against, created through the address the bank's own router publishes.
async fn connection(graph: &Arc<MemoryGraph>) -> String {
    let (status, body) = call(
        Arc::clone(graph),
        Method::POST,
        "/connections",
        Some(json!({
            "provider": "manual",
            "account_name": "FNB Business",
            "account_number_last4": "4321",
            "today": "2026-11-14",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["id"].as_str().expect("the connection's id").to_owned()
}

#[tokio::test]
async fn a_single_transaction_is_imported_at_the_address_the_service_published() {
    let graph = Arc::new(MemoryGraph::new());
    let connection_id = connection(&graph).await;

    let (status, body) = call(
        Arc::clone(&graph),
        Method::POST,
        "/bank-feed/transactions/import",
        Some(json!({
            "bank_connection_id": connection_id,
            "external_id": "fnb-0001",
            "date": "2026-11-02",
            "amount": "1250.00",
            "description": "Customer deposit",
            "transaction_type": "credit",
            "today": "2026-11-14",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["external_id"], "fnb-0001");
    assert_eq!(
        amount_of(&body),
        "1250",
        "the amount is kept exactly: {body}"
    );

    // the same statement line arriving twice is refused, not imported twice
    let (status, again) = call(
        graph,
        Method::POST,
        "/bank-feed/transactions/import",
        Some(json!({
            "bank_connection_id": body["bank_connection_id"],
            "external_id": "fnb-0001",
            "date": "2026-11-02",
            "amount": "1250.00",
            "today": "2026-11-14",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{again}");
}

#[tokio::test]
async fn a_batch_reports_each_line_it_imported() {
    let graph = Arc::new(MemoryGraph::new());
    let connection_id = connection(&graph).await;

    let (status, body) = call(
        graph,
        Method::POST,
        "/bank-feed/transactions/import-batch",
        Some(json!({
            "bank_connection_id": connection_id,
            "transactions": [
                {"external_id": "fnb-b1", "date": "2026-11-03", "amount": "99.50", "description": "Fuel"},
                {"external_id": "fnb-b2", "date": "2026-11-04", "amount": "410.00", "description": "Rent"},
            ],
            "today": "2026-11-14",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let imported = body["imported"]
        .as_array()
        .or_else(|| body["results"].as_array())
        .expect("the lines that were imported");
    assert_eq!(imported.len(), 2, "{body}");
}

#[tokio::test]
async fn an_mt940_statement_is_parsed_into_the_transactions_it_holds() {
    let graph = Arc::new(MemoryGraph::new());
    let connection_id = connection(&graph).await;

    let statement = ":20:STMT2026\n\
                     :61:2611021102C1250,00NTRFNONREF\n\
                     :86:Customer deposit\n\
                     :61:2611041104D410,00NTRFNONREF\n\
                     :86:Rent payment\n";
    let (status, body) = call(
        Arc::clone(&graph),
        Method::POST,
        "/bank-feed/transactions/import-mt940",
        Some(json!({
            "connection_id": connection_id,
            "statement_data": statement,
            "today": "2026-11-14",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["parsed"], 2, "two statement lines were read: {body}");
    assert_eq!(body["imported"], 2, "and both were kept: {body}");

    // the mark decides the side, so the credit is positive and the debit negative
    let ids = body["transaction_ids"]
        .as_array()
        .expect("the transactions it wrote");
    let (_, credit) = call(
        Arc::clone(&graph),
        Method::GET,
        &format!("/bank-transactions/{}", ids[0].as_str().unwrap()),
        None,
    )
    .await;
    let (_, debit) = call(
        Arc::clone(&graph),
        Method::GET,
        &format!("/bank-transactions/{}", ids[1].as_str().unwrap()),
        None,
    )
    .await;
    assert_eq!(amount_of(&credit), "1250", "{credit}");
    assert_eq!(amount_of(&debit), "-410", "{debit}");
    assert_eq!(debit["date"], "2026-11-04");
}

#[tokio::test]
async fn a_rules_are_applied_to_the_transactions_named() {
    let graph = Arc::new(MemoryGraph::new());
    let connection_id = connection(&graph).await;

    let (status, rule) = call(
        Arc::clone(&graph),
        Method::POST,
        "/bank-rules",
        Some(json!({
            "name": "Fuel",
            "match_conditions": {"description_contains": "Fuel"},
            "auto_match_enabled": true,
            "today": "2026-11-14",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{rule}");

    let (_, imported) = call(
        Arc::clone(&graph),
        Method::POST,
        "/bank-feed/transactions/import",
        Some(json!({
            "bank_connection_id": connection_id,
            "external_id": "fnb-rule-1",
            "date": "2026-11-05",
            "amount": "88.00",
            "description": "Fuel at Engen",
            "today": "2026-11-14",
        })),
    )
    .await;
    let transaction_id = imported["id"]
        .as_str()
        .expect("the transaction id")
        .to_owned();

    let (status, applied) = call(
        Arc::clone(&graph),
        Method::POST,
        "/bank-feed/rules/apply",
        Some(json!([transaction_id])),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{applied}");
    // the rule that names Fuel picks the fuel line out, and its outcome is reported per transaction
    assert_eq!(applied["processed"], 1, "{applied}");
    assert_eq!(
        applied["results"][0]["matched"], true,
        "the fuel rule met it: {applied}"
    );
    assert_eq!(applied["results"][0]["rule_id"], rule["id"], "{applied}");
}

#[tokio::test]
async fn the_feed_is_listed_and_a_transaction_status_set_at_the_published_addresses() {
    let graph = Arc::new(MemoryGraph::new());
    let connection_id = connection(&graph).await;

    let (_, imported) = call(
        Arc::clone(&graph),
        Method::POST,
        "/bank-feed/transactions/import",
        Some(json!({
            "bank_connection_id": connection_id,
            "external_id": "fnb-list-1",
            "date": "2026-11-06",
            "amount": "300.00",
            "description": "Card settlement",
            "today": "2026-11-14",
        })),
    )
    .await;
    let id = imported["id"]
        .as_str()
        .expect("the transaction id")
        .to_owned();

    let (status, listed) = call(
        Arc::clone(&graph),
        Method::GET,
        "/bank-feed/transactions",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    assert!(
        listed.to_string().contains("fnb-list-1"),
        "the line just imported is in the feed: {listed}"
    );

    let (status, moved) = call(
        Arc::clone(&graph),
        Method::PUT,
        &format!("/bank-feed/transactions/{id}/status"),
        Some(json!({"status": "reconciled", "today": "2026-11-14"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{moved}");
    assert_eq!(moved["status"], "reconciled");
}
