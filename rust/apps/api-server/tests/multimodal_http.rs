#![forbid(unsafe_code)]
//! multimodal-pipeline-service over HTTP: a capture, the inbox by state, and a correction.

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use serde_json::{json, Value};
use std::sync::Arc;
use store::fake::MemoryGraph;
use tower::ServiceExt;

fn router() -> axum::Router {
    vimbai_api::records::router(Arc::new(MemoryGraph::new()))
}

async fn call(
    router: &axum::Router,
    method: Method,
    uri: &str,
    payload: Value,
) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .header("x-user-id", "user-1")
        .header("x-book-id", "book-1")
        .body(Body::from(payload.to_string()))
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
async fn a_capture_waits_in_the_inbox_until_someone_accepts_or_corrects_it() {
    let router = router();

    let (status, made) = call(
        &router,
        Method::POST,
        "/tasks",
        json!({"input_type": "document", "input_url": "https://files.test/receipt.jpg"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{made}");
    assert_eq!(made["status"], json!("received"));
    // nothing here read the document, and the answer says so rather than leaving a caller to poll
    assert_eq!(made["reading_queued"], json!(false));
    let id = made["id"].as_str().expect("an id").to_string();

    // a sixth kind of input is refused as unprocessable
    let (status, _) = call(
        &router,
        Method::POST,
        "/tasks",
        json!({"input_type": "hologram"}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    // the inbox holds what is not yet accepted
    let (status, open) = call(&router, Method::GET, "/inbox", json!({})).await;
    assert_eq!(status, StatusCode::OK, "{open}");
    assert_eq!(open.as_array().map(Vec::len), Some(1));

    // a fourth view is understood and refused, and the limit is held between one and five hundred
    let (status, _) = call(&router, Method::GET, "/inbox?view=everything", json!({})).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let (status, held) = call(&router, Method::GET, "/inbox?view=all&limit=0", json!({})).await;
    assert_eq!(status, StatusCode::OK, "{held}");
    assert_eq!(held.as_array().map(Vec::len), Some(1));

    // the badge counts each state
    let (status, summary) = call(&router, Method::GET, "/inbox/summary", json!({})).await;
    assert_eq!(status, StatusCode::OK, "{summary}");
    assert_eq!(summary["received"], json!(1));
    assert_eq!(summary["total_incomplete"], json!(1));
    assert_eq!(summary["total_organized"], json!(0));

    // a correction marks the capture corrected, and keeps the value it started from
    let (status, corrected) = call(
        &router,
        Method::POST,
        &format!("/tasks/{id}/corrections"),
        json!({"field_name": "total_amount", "original_value": "41.32", "corrected_value": "43.12"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{corrected}");
    assert_eq!(corrected["original_value"], json!("41.32"));
    let (_, after) = call(&router, Method::GET, &format!("/tasks/{id}"), json!({})).await;
    assert_eq!(after["status"], json!("user_corrected"));

    // accepting it means posting the entry the reading proposed, so the capture needs a reading and the Book the accounts
    // it names: an unread capture is refused by name, which its own test covers
    for (number, name, kind) in [("1000", "Cash", "asset"), ("5000", "Groceries", "expense")] {
        let (status, made) = call(
            &router,
            Method::POST,
            "/accounting/accounts",
            json!({"account_number": number, "name": name, "account_type": kind}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{made}");
    }
    let (_, read) = call(
        &router,
        Method::PUT,
        &format!("/tasks/{id}"),
        json!({
            "status": "ai_extracted",
            "suggested_journal_entry": {
                "entry_date": "2026-11-17",
                "description": "Receipt",
                "lines": [
                    {"account_number": "5000", "debit": "41.32", "credit": "0"},
                    {"account_number": "1000", "debit": "0", "credit": "41.32"}
                ]
            }
        }),
    )
    .await;
    assert_eq!(read["status"], json!("ai_extracted"), "{read}");

    // accepting it twice changes nothing, keeping the first finishing time
    let (status, first) = call(
        &router,
        Method::POST,
        &format!("/inbox/{id}/organize"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{first}");
    assert_eq!(first["status"], json!("completed"));
    let first_end = first["processing_end_time"].clone();
    let (_, again) = call(
        &router,
        Method::POST,
        &format!("/inbox/{id}/organize"),
        json!({"now": 9999}),
    )
    .await;
    assert_eq!(again["processing_end_time"], first_end);

    // accepted records move to the organized view
    let (_, organized) = call(&router, Method::GET, "/inbox?view=organized", json!({})).await;
    assert_eq!(organized.as_array().map(Vec::len), Some(1));

    // a correction on a capture that is not there is missing
    let (status, _) = call(
        &router,
        Method::POST,
        "/tasks/nope/corrections",
        json!({"field_name": "x"}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // a delete removes it, and deleting again is missing
    let (status, gone) = call(&router, Method::DELETE, &format!("/tasks/{id}"), json!({})).await;
    assert_eq!(status, StatusCode::OK, "{gone}");
    let (status, _) = call(&router, Method::DELETE, &format!("/tasks/{id}"), json!({})).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

/// A capture nothing has read cannot be accepted into the Book.
///
/// Accepting used to mean flipping the record to `completed`, which took it out of the inbox while posting nothing: the
/// record vanished and the statements moved by zero. The refusal names the situation instead, and the capture stays where
/// a person can still act on it.
#[tokio::test]
async fn an_unread_capture_is_refused_rather_than_accepted_into_nothing() {
    let router = router();
    let (_, made) = call(
        &router,
        Method::POST,
        "/tasks",
        json!({"input_type": "image", "input_url": "data:image/png;base64,iVBORw0KGgo="}),
    )
    .await;
    let id = made["id"].as_str().expect("an id").to_string();

    let (status, refused) = call(
        &router,
        Method::POST,
        &format!("/inbox/{id}/organize"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{refused}");
    assert!(
        refused["error"]
            .as_str()
            .unwrap_or_default()
            .contains("has not been read yet"),
        "{refused}"
    );

    // it is still the inbox's to show, and nothing was posted
    let (_, summary) = call(&router, Method::GET, "/inbox/summary", json!({})).await;
    assert_eq!(summary["total_incomplete"], json!(1));
    assert_eq!(summary["total_organized"], json!(0));
    let (_, journal) = call(
        &router,
        Method::GET,
        "/accounting/journal-entries",
        json!({}),
    )
    .await;
    assert_eq!(journal.as_array().map(Vec::len), Some(0), "{journal}");
}

/// A capture that carries a reading is accepted by posting the entry the reading proposed.
///
/// The posting goes through the ledger's own door, so it is validated and stamped like any other: this is how a receipt
/// photographed on a phone reaches the statements, and accepting it twice must not post it twice.
#[tokio::test]
async fn a_capture_with_a_reading_posts_the_entry_it_proposed() {
    let router = router();
    for (number, name, kind) in [("1000", "Cash", "asset"), ("5000", "Groceries", "expense")] {
        let (status, made) = call(
            &router,
            Method::POST,
            "/accounting/accounts",
            json!({"account_number": number, "name": name, "account_type": kind}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{made}");
    }

    let (_, made) = call(
        &router,
        Method::POST,
        "/tasks",
        json!({"input_type": "image", "input_url": "data:image/png;base64,iVBORw0KGgo="}),
    )
    .await;
    let id = made["id"].as_str().expect("an id").to_string();

    let (status, read) = call(
        &router,
        Method::PUT,
        &format!("/tasks/{id}"),
        json!({
            "status": "ai_extracted",
            "suggested_journal_entry": {
                "entry_date": "2026-11-17",
                "description": "Groceries from the receipt",
                "reference_number": "REC-1",
                "lines": [
                    {"account_number": "5000", "debit": "250.00", "credit": "0"},
                    {"account_number": "1000", "debit": "0", "credit": "250.00"}
                ]
            }
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{read}");

    let (status, accepted) = call(
        &router,
        Method::POST,
        &format!("/inbox/{id}/organize"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{accepted}");
    assert_eq!(accepted["status"], json!("completed"));
    let entry_id = accepted["linked_vimbai_entity_id"]
        .as_str()
        .expect("the entry it became")
        .to_string();

    // the entry is in the journal, posted, and the statements moved by it
    let (_, entry) = call(
        &router,
        Method::GET,
        &format!("/accounting/journal-entries/{entry_id}"),
        json!({}),
    )
    .await;
    assert_eq!(entry["status"], json!("posted"), "{entry}");
    assert_eq!(entry["description"], json!("Groceries from the receipt"));
    let (_, trial) = call(&router, Method::GET, "/accounting/trial-balance", json!({})).await;
    let moved: Vec<Value> = trial["accounts"]
        .as_array()
        .expect("accounts")
        .iter()
        .filter(|row| row["debit"] != json!("0.000000") || row["credit"] != json!("0.000000"))
        .cloned()
        .collect();
    assert_eq!(moved.len(), 2, "{trial}");
    assert_eq!(moved[0]["account_number"], json!("1000"));
    assert_eq!(moved[0]["credit"], json!("250.000000"));
    assert_eq!(moved[1]["account_number"], json!("5000"));
    assert_eq!(moved[1]["debit"], json!("250.000000"));

    // accepting it again posts nothing a second time
    let (status, again) = call(
        &router,
        Method::POST,
        &format!("/inbox/{id}/organize"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{again}");
    let (_, journal) = call(
        &router,
        Method::GET,
        "/accounting/journal-entries",
        json!({}),
    )
    .await;
    assert_eq!(journal.as_array().map(Vec::len), Some(1), "{journal}");

    // and the chain still verifies over what was posted
    let (status, integrity) = call(
        &router,
        Method::GET,
        "/accounting/ledger/integrity",
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{integrity}");
    assert_eq!(integrity["valid"], json!(true), "{integrity}");
}
