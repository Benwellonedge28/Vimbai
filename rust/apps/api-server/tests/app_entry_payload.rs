#![forbid(unsafe_code)]

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
    fn stranger() -> Self {
        Self {
            router: vimbai_api::records::router(Arc::new(MemoryGraph::new())),
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
}

/// The journal form's own payload, posted where the form posts it, answered in the shape the form parses.
///
/// `createJournalEntry` posts `JournalEntry.toJson()` and then parses the reply with `JournalEntry.fromJson`, which declares
/// `debit` and `credit` as `double`. The engine answers `"250.000000"` - a string, because an exact decimal does not survive
/// a JSON number - so the post succeeded and the phone threw while reading its own success. This holds both halves: the
/// engine keeps answering the form's payload, and the money it answers with is a decimal string the app now reads.
#[tokio::test]
async fn the_journal_form_posts_a_payload_the_engine_answers_and_can_read_back() {
    let mut walk = Walk::stranger();
    let (status, made) = walk
        .call(
            Method::POST,
            "/identity/users/register",
            Some(json!({"email": "probe@example.com", "password": "Str0ng-pass-9", "first_name": "Probe"})),
        )
        .await;
    assert!(status.is_success(), "{made}");
    walk.user = made["id"].as_str().expect("an id").to_string();
    let (_, book) = walk
        .call(
            Method::POST,
            "/books",
            Some(json!({"name": "Probe", "currency": "USD", "tier": "business"})),
        )
        .await;
    walk.book = book["book"]["id"].as_str().unwrap().to_string();

    let (code, reply) = walk
        .call(
            Method::POST,
            "/accounting/journal-entries",
            Some(json!({
                "id": "3f2b6a1e-0000-4000-8000-000000000000",
                "entry_date": "2026-11-15T20:41:00.123456",
                "description": "first sale from the phone",
                "reference_number": "INV-1",
                "source_module": "MobileApp",
                "status": null,
                "lines": [
                    {"account_number": "1000", "description": null, "debit": 250.0, "credit": 0.0},
                    {"account_number": "4000", "description": null, "debit": 0.0, "credit": 250.0}
                ],
                "created_at": null,
                "updated_at": null
            })),
        )
        .await;
    assert_eq!(code, StatusCode::OK, "{reply}");
    assert_eq!(
        reply["description"],
        json!("first sale from the phone"),
        "{reply}"
    );
    // what the form writes is kept, and an entry the form does not mark posted stays a draft rather than moving money
    assert_eq!(reply["source_module"], json!("MobileApp"), "{reply}");
    assert_eq!(reply["reference_number"], json!("INV-1"), "{reply}");
    assert_eq!(reply["status"], json!("draft"), "{reply}");
    assert!(
        !reply["id"].as_str().unwrap_or_default().is_empty(),
        "{reply}"
    );
    assert!(reply["entry_date"].is_string(), "{reply}");
    assert!(reply["created_at"].is_string(), "{reply}");
    let lines = reply["lines"].as_array().expect("lines");
    assert_eq!(lines.len(), 2, "{reply}");
    // the money is a decimal string, not a float: this is the shape that used to throw on the phone
    for line in lines {
        assert!(line["account_number"].is_string(), "{line}");
        let debit = line["debit"].as_str().unwrap_or_else(|| panic!("{line}"));
        let credit = line["credit"].as_str().unwrap_or_else(|| panic!("{line}"));
        assert!(debit.parse::<f64>().is_ok(), "{line}");
        assert!(credit.parse::<f64>().is_ok(), "{line}");
    }
    let posted: f64 = lines
        .iter()
        .map(|line| line["debit"].as_str().unwrap().parse::<f64>().unwrap())
        .sum();
    assert_eq!(posted, 250.0, "{reply}");
}
