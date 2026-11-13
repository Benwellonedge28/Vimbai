//! graphql-service over HTTP: one POST body carrying a query, answered field by field.

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use serde_json::{json, Value};
use std::sync::Arc;
use store::fake::MemoryGraph;
use tower::ServiceExt;

fn router() -> axum::Router {
    vimbai_api::records::router(Arc::new(MemoryGraph::new()))
}

async fn ask(router: &axum::Router, body: Value) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(Method::POST)
        .uri("/graphql")
        .header("content-type", "application/json")
        .header("x-user-id", "user-1")
        .header("x-book-id", "book-1")
        .body(Body::from(body.to_string()))
        .unwrap();
    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1_000_000)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[tokio::test]
async fn a_query_answers_the_fields_it_asks_for_and_a_journal_entry_must_balance() {
    let router = router();

    let (status, answer) = ask(
        &router,
        json!({"query": "{ accounts { id name balance } currencies { code } }"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{answer}");
    assert_eq!(answer["data"]["accounts"][0]["id"], json!("acc-001"));
    assert!(answer["data"]["accounts"][0]
        .get("account_number")
        .is_none());
    assert_eq!(answer["data"]["currencies"][2]["code"], json!("GBP"));

    // a rate is answered where one is known, and a pair the service never knew has none
    let (_, rates) = ask(
        &router,
        json!({"query": "{ known: exchange_rate(from_currency: \"USD\", to_currency: \"GBP\") }"}),
    )
    .await;
    // an alias is refused rather than answered under a name the client chose
    assert!(rates["errors"][0]["message"].as_str().is_some(), "{rates}");
    let (_, rate) = ask(
        &router,
        json!({"query": "{ exchange_rate(from_currency: \"USD\", to_currency: \"GBP\") }"}),
    )
    .await;
    assert_eq!(rate["data"]["exchange_rate"], json!(0.79));

    // a conversion answers in exact money, and an unknown pair converts at one as shipped
    let (_, converted) = ask(
        &router,
        json!({"query": "mutation { convert_currency(input: {from_currency: \"USD\", to_currency: \"ZAR\", amount: 12.5}) { converted_amount rate_used } }"}),
    )
    .await;
    assert_eq!(
        converted["data"]["convert_currency"]["converted_amount"],
        json!(12.5)
    );
    assert_eq!(
        converted["data"]["convert_currency"]["rate_used"],
        json!(1.0)
    );

    // a journal entry that balances is accepted, and one a hundredth of a cent out is refused
    let balanced = "mutation { create_journal_entry(input: {entry_date: \"2026-11-13\", description: \"d\", \
                    source_module: \"manual\", lines: [{account_number: \"1001\", description: \"Cash\", \
                    debit: 100, credit: 0}, {account_number: \"4000\", description: \"Revenue\", debit: 0, \
                    credit: 100}]}) { success id } }";
    let (_, accepted) = ask(&router, json!({"query": balanced})).await;
    assert_eq!(
        accepted["data"]["create_journal_entry"]["success"],
        json!(true)
    );

    let drifted = "mutation { create_journal_entry(input: {entry_date: \"2026-11-13\", description: \"d\", \
                   source_module: \"manual\", lines: [{account_number: \"1001\", description: \"Cash\", \
                   debit: 100.01, credit: 0}, {account_number: \"4000\", description: \"Revenue\", debit: 0, \
                   credit: 100}]}) { success message } }";
    let (_, refused) = ask(&router, json!({"query": drifted})).await;
    assert_eq!(
        refused["data"]["create_journal_entry"]["success"],
        json!(false)
    );

    // a variable stands for the argument it was sent for
    let (status, notified) = ask(
        &router,
        json!({"query": "query Inbox($who: String) { notifications(user_id: $who) { user_id is_read } }",
               "variables": {"who": "user-1"}}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{notified}");
    assert_eq!(
        notified["data"]["notifications"][0]["user_id"],
        json!("user-1")
    );

    // a field outside the schema answers an error rather than a guess
    let (_, unknown) = ask(&router, json!({"query": "{ trial_balance { total } }"})).await;
    assert!(unknown["errors"][0]["message"]
        .as_str()
        .unwrap_or_default()
        .contains("trial_balance"));

    // and a request carrying no query at all is refused before anything is read
    let (status, _) = ask(&router, json!({"variables": {}})).await;
    assert_ne!(status, StatusCode::OK);
}
