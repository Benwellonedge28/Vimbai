#![forbid(unsafe_code)]
//! The first run: a stranger arrives, opens a Book, and the bookkeeping loop answers with their money in it.
//!
//! Everything else in this engine is reachable only after this walk succeeds, so it is tested as one walk rather than as
//! steps that each pass on their own. Registering an account, signing in, creating a Book, naming accounts and posting an
//! entry are separate services; the thing a person experiences is the whole path, and it broke exactly there. The device
//! store returned each listed entry one level down - `{"x": {...entry}, "lines": [...]}` - so a Book full of posted entries
//! reported a trial balance of zeroes. The unit tests passed because they read the lines, which were where they were
//! promised. Nothing read the entry's own fields, which were not.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use serde_json::{json, Value};
use store::fake::MemoryGraph;
use tower::ServiceExt;

struct Walk {
    router: axum::Router,
    user: String,
    book: String,
}

impl Walk {
    /// A stranger, with no account and no Book.
    fn stranger() -> Self {
        let graph = Arc::new(MemoryGraph::new());
        Self {
            router: vimbai_api::records::router(graph),
            user: String::new(),
            book: String::new(),
        }
    }

    async fn call(&self, method: Method, path: &str, body: Option<Value>) -> (StatusCode, Value) {
        let mut builder = Request::builder()
            .method(method)
            .uri(path)
            .header("content-type", "application/json");
        if !self.user.is_empty() {
            builder = builder.header("x-user-id", &self.user);
        }
        if !self.book.is_empty() {
            builder = builder.header("x-book-id", &self.book);
        }
        let body = match body {
            Some(value) => Body::from(value.to_string()),
            None => Body::empty(),
        };
        let response = self
            .router
            .clone()
            .oneshot(builder.body(body).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 4_000_000)
            .await
            .unwrap();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    async fn register_and_sign_in(&mut self) -> String {
        let (status, made) = self
            .call(
                Method::POST,
                "/identity/users/register",
                Some(json!({
                    "email": "stranger@example.com",
                    "password": "Str0ng-pass-9",
                    "first_name": "Stan",
                })),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "{made}");
        self.user = made["id"].as_str().expect("an id").to_string();

        let (status, signed) = self
            .call(
                Method::POST,
                "/identity/users/login",
                Some(json!({"identifier": "stranger@example.com", "password": "Str0ng-pass-9"})),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{signed}");
        signed["access_token"]
            .as_str()
            .expect("a token")
            .to_string()
    }

    async fn open_book(&mut self) -> Value {
        let (status, made) = self
            .call(
                Method::POST,
                "/books",
                Some(json!({"name": "Stan's books", "currency": "USD", "tier": "business"})),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{made}");
        self.book = made["book"]["id"].as_str().expect("a book id").to_string();
        made
    }

    /// The accounts the Book holds, as `GET /accounting/accounts` answers them.
    async fn chart(&self) -> Vec<Value> {
        let (status, listed) = self.call(Method::GET, "/accounting/accounts", None).await;
        assert_eq!(status, StatusCode::OK, "{listed}");
        listed
            .as_array()
            .cloned()
            .or_else(|| listed["accounts"].as_array().cloned())
            .expect("a chart")
    }

    async fn post_entry(&self, status: &str, amount: &str) -> Value {
        let (code, made) = self
            .call(
                Method::POST,
                "/accounting/journal-entries",
                Some(json!({
                    "entry_date": "2026-11-15",
                    "description": "first sale",
                    "status": status,
                    "lines": [
                        {"account_number": "1000", "debit": amount, "credit": "0"},
                        {"account_number": "4000", "debit": "0", "credit": amount},
                    ],
                })),
            )
            .await;
        assert_eq!(code, StatusCode::OK, "{made}");
        made
    }

    async fn trial_balance(&self) -> Value {
        let (status, answered) = self
            .call(Method::GET, "/accounting/trial-balance", None)
            .await;
        assert_eq!(status, StatusCode::OK, "{answered}");
        answered
    }
}

#[tokio::test]
async fn a_stranger_can_open_a_book_and_its_first_entry_moves_the_balance() {
    let mut walk = Walk::stranger();
    let token = walk.register_and_sign_in().await;
    assert!(!token.is_empty(), "signing in has to mint a token");

    let opened = walk.open_book().await;
    // the person who opened the Book owns it
    assert_eq!(opened["your_role"], json!("owner"));
    assert!(!walk.book.is_empty(), "opening a Book has to name it");

    // the Book opens with the accounts it takes to write in it, so the first sale needs no setup
    let chart = walk.chart().await;
    assert!(
        chart
            .iter()
            .any(|row| row["account_number"] == json!("1000")),
        "a new Book opens with cash: {chart:?}"
    );

    walk.post_entry("posted", "250.00").await;

    let balance = walk.trial_balance().await;
    assert_eq!(
        balance["total_debits"],
        json!("250.000000"),
        "the sale has to reach the trial balance: {balance}"
    );
    assert_eq!(balance["total_credits"], json!("250.000000"), "{balance}");
    assert_eq!(balance["is_balanced"], json!(true), "{balance}");

    let rows = balance["accounts"].as_array().expect("rows");
    let cash = rows
        .iter()
        .find(|row| row["account_number"] == json!("1000"))
        .expect("the cash row");
    assert_eq!(cash["debit"], json!("250.000000"), "{balance}");
}

#[tokio::test]
async fn a_listed_entry_says_what_it_is_without_being_unwrapped_first() {
    let mut walk = Walk::stranger();
    walk.register_and_sign_in().await;
    walk.open_book().await;
    walk.post_entry("posted", "10.00").await;

    let (status, listed) = walk
        .call(Method::GET, "/accounting/journal-entries", None)
        .await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    let entries = listed
        .as_array()
        .or_else(|| listed["journal_entries"].as_array())
        .expect("a listing");
    let entry = entries.first().expect("the entry just posted");

    // the entry's own fields sit on the entry, not one level down inside it
    assert_eq!(
        entry["status"],
        json!("posted"),
        "an entry has to carry its own status: {entry}"
    );
    assert_eq!(entry["entry_date"], json!("2026-11-15"), "{entry}");
    assert_eq!(entry["description"], json!("first sale"), "{entry}");
    // and its lines arrive with it
    assert_eq!(entry["lines"].as_array().map(Vec::len), Some(2), "{entry}");
}

#[tokio::test]
async fn a_draft_entry_is_kept_and_kept_out_of_the_statements() {
    let mut walk = Walk::stranger();
    walk.register_and_sign_in().await;
    walk.open_book().await;
    let draft = walk.post_entry("draft", "99.00").await;
    assert_eq!(draft["status"], json!("draft"), "{draft}");

    // a draft is a bookkeeper's note, not a posting: the statements do not move until it is posted
    let balance = walk.trial_balance().await;
    assert_eq!(balance["total_debits"], json!("0.000000"), "{balance}");
    assert_eq!(balance["total_credits"], json!("0.000000"), "{balance}");

    // and once posted, the same entry counts
    walk.post_entry("posted", "99.00").await;
    let balance = walk.trial_balance().await;
    assert_eq!(balance["total_debits"], json!("99.000000"), "{balance}");
}
