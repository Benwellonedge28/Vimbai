//! notifications-service over HTTP: a notice into each recipient's inbox, that person's own reading, and preferences.

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

    async fn call(
        &self,
        method: Method,
        uri: &str,
        payload: Value,
        user: &str,
    ) -> (StatusCode, Value) {
        let request = Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json")
            .header("x-user-id", user)
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

    async fn get(&self, uri: &str, user: &str) -> (StatusCode, Value) {
        let request = Request::builder()
            .method(Method::GET)
            .uri(uri)
            .header("x-user-id", user)
            .header("x-book-id", "book-1")
            .body(Body::empty())
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

#[tokio::test]
async fn a_notice_lands_in_the_recipients_inbox_and_is_theirs_to_read() {
    let book = Book::new();
    let (status, sent) = book
        .call(
            Method::POST,
            "/notifications",
            json!({
                "type": "approval_required",
                "title": "Invoice needs approval",
                "message": "INV-1 is waiting",
                "recipients": ["user-2"],
                "created_at": "2026-01-02T09:00:00Z"
            }),
            "user-1",
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{sent}");
    assert_eq!(sent["sender"], json!("user-1"));

    // the recipient finds it in their own inbox
    let (status, theirs) = book.get("/notifications/user-2", "user-2").await;
    assert_eq!(status, StatusCode::OK, "{theirs}");
    assert_eq!(theirs["total_count"], json!(1));
    assert_eq!(theirs["unread_count"], json!(1));

    // somebody else asking for that inbox is refused outright
    let (status, refused) = book.get("/notifications/user-2", "user-1").await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{refused}");

    let id = theirs["notifications"][0]["id"].as_str().expect("an id");
    let (status, read) = book
        .call(
            Method::PUT,
            &format!("/notifications/{id}/read"),
            json!({"read_at": "2026-01-03T09:00:00Z"}),
            "user-2",
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{read}");
    let (_, after) = book
        .get("/notifications/user-2?unread_only=true", "user-2")
        .await;
    assert_eq!(
        after["notifications"].as_array().map(|held| held.len()),
        Some(0)
    );

    // the sender never had it, so it is not theirs to delete
    let (status, missing) = book
        .call(
            Method::DELETE,
            &format!("/notifications/{id}"),
            json!({}),
            "user-1",
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{missing}");

    let (status, gone) = book
        .call(
            Method::DELETE,
            &format!("/notifications/{id}"),
            json!({}),
            "user-2",
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{gone}");
}

#[tokio::test]
async fn a_template_fills_in_and_preferences_are_personal() {
    let book = Book::new();
    let (status, listed) = book.get("/notification-templates", "user-1").await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    assert_eq!(listed.as_array().map(|held| held.len()), Some(5));

    let (status, _) = book.get("/notification-templates/invented", "user-1").await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, sent) = book
        .call(
            Method::POST,
            "/notification-templates/comment_added/send",
            json!({
                "recipients": ["user-1"],
                "variables": {"sender": "Tariro", "title": "Invoice 7", "comment": "looks wrong"},
                "created_at": "2026-01-02T09:00:00Z"
            }),
            "user-1",
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{sent}");
    assert_eq!(sent["title"], json!("Tariro commented on Invoice 7"));

    // the defaults until a person sets their own
    let (status, defaults) = book.get("/preferences/user-1", "user-1").await;
    assert_eq!(status, StatusCode::OK, "{defaults}");
    assert_eq!(defaults["email_batch_interval_minutes"], json!(60));

    let (status, saved) = book
        .call(
            Method::PUT,
            "/preferences/user-1",
            json!({"email_batch": false, "quiet_hours_start": "20:00"}),
            "user-1",
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{saved}");
    assert_eq!(saved["preferences"]["email_batch"], json!(false));

    let (status, refused) = book.get("/preferences/user-1", "user-2").await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{refused}");

    let (status, counted) = book.get("/notification-stats", "user-1").await;
    assert_eq!(status, StatusCode::OK, "{counted}");
    assert_eq!(counted["unread_notifications"], json!(1));
    assert_eq!(counted["active_connections"], json!(0));
}

#[tokio::test]
async fn a_workflow_and_an_approval_each_tell_the_right_person() {
    let book = Book::new();
    let (status, started) = book
        .call(
            Method::POST,
            "/workflow/wf-1/notify",
            json!({"event_type": "started", "participants": ["user-2"],
                   "created_at": "2026-01-02T09:00:00Z"}),
            "user-1",
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{started}");
    // only a completion reads as a completed workflow
    assert_eq!(started["type"], json!("workflow_failed"));

    let (status, asked) = book
        .call(
            Method::POST,
            "/approval/ap-1/notify",
            json!({"event_type": "requested", "requester": "user-1", "approvers": ["user-2"],
                   "created_at": "2026-01-02T10:00:00Z"}),
            "user-1",
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{asked}");
    assert_eq!(asked["priority"], json!("high"));

    // the approver holds both notices, the requester none of these two
    let (_, approver) = book.get("/notifications/user-2", "user-2").await;
    assert_eq!(approver["total_count"], json!(2));
    let (_, requester) = book.get("/notifications/user-1", "user-1").await;
    assert_eq!(requester["total_count"], json!(0));
}
