#![forbid(unsafe_code)]
//! alerts-service over HTTP: a rule set, a watch that fires inside its cooldown rules, and an alert settled.

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

    async fn get(&self, uri: &str) -> (StatusCode, Value) {
        let request = Request::builder()
            .method(Method::GET)
            .uri(uri)
            .header("x-user-id", "user-1")
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
async fn a_rule_fires_a_watch_waits_out_its_cooldown_and_the_alert_is_settled() {
    let book = Book::new();
    let (status, rule) = book
        .call(
            Method::POST,
            "/rules",
            json!({
                "name": "Large payment",
                "category": "fraud",
                "severity": "critical",
                "condition": {"type": "threshold", "field": "amount", "operator": "gt", "value": 10000},
                "cooldown_seconds": 3600,
                "created_at": "2026-01-01T09:00:00Z"
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{rule}");
    assert_eq!(rule["enabled"], json!(true));

    // a name too short is refused before anything is stored
    let (status, _) = book
        .call(
            Method::POST,
            "/rules",
            json!({"name": "no", "category": "fraud", "severity": "critical"}),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    let (status, fired) = book
        .call(
            Method::POST,
            "/alerts/evaluate",
            json!({"data": {"amount": 12500, "source": "ledger"}, "now": "2026-01-02T09:00:00Z"}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{fired}");
    assert_eq!(fired["triggered_count"], json!(1));
    assert_eq!(
        fired["alerts"][0]["title"],
        json!("Large payment: Threshold exceeded")
    );

    // inside the cooldown nothing new is raised
    let (status, held) = book
        .call(
            Method::POST,
            "/alerts/evaluate",
            json!({"data": {"amount": 12500}, "now": "2026-01-02T09:30:00Z"}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{held}");
    assert_eq!(held["triggered_count"], json!(0));

    let alert_id = fired["alerts"][0]["id"].as_str().expect("an id");
    let (status, acknowledged) = book
        .call(
            Method::PUT,
            &format!("/alerts/{alert_id}/acknowledge"),
            json!({"acknowledged_at": "2026-01-02T10:00:00Z"}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{acknowledged}");
    assert_eq!(acknowledged["status"], json!("acknowledged"));

    let (status, resolved) = book
        .call(
            Method::PUT,
            &format!("/alerts/{alert_id}/resolve"),
            json!({"resolved_at": "2026-01-02T11:00:00Z"}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{resolved}");
    assert_eq!(resolved["status"], json!("resolved"));

    let (status, counted) = book.get("/alerts/stats").await;
    assert_eq!(status, StatusCode::OK, "{counted}");
    assert_eq!(counted["total_alerts"], json!(1));
    assert_eq!(counted["by_status"]["resolved"], json!(1));
    assert_eq!(counted["enabled_rules"], json!(1));
}

#[tokio::test]
async fn a_rule_can_be_shouted_through_even_while_it_is_switched_off() {
    let book = Book::new();
    let (status, rule) = book
        .call(
            Method::POST,
            "/rules",
            json!({
                "name": "Cash variance",
                "category": "financial",
                "severity": "medium",
                "condition": {"type": "deadline", "days_before": 3},
                "enabled": false,
                "created_at": "2026-01-01T09:00:00Z"
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{rule}");
    let rule_id = rule["id"].as_str().expect("an id");

    // a switched off rule is not watched
    let (status, watched) = book
        .call(
            Method::POST,
            "/alerts/evaluate",
            json!({"data": {"deadline": "2026-01-04"}, "now": "2026-01-02T09:00:00Z"}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{watched}");
    assert_eq!(watched["triggered_count"], json!(0));

    // but a service can still shout through it
    let (status, shouted) = book
        .call(
            Method::POST,
            &format!("/trigger/{rule_id}"),
            json!({"message": "Drawer is short", "now": "2026-01-02T09:00:00Z"}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{shouted}");
    assert_eq!(shouted["title"], json!("Cash variance"));
    assert_eq!(shouted["source"], json!("internal"));

    let (status, dismissed) = book
        .call(
            Method::PUT,
            &format!("/alerts/{}/dismiss", shouted["id"].as_str().expect("an id")),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{dismissed}");
    assert_eq!(dismissed["status"], json!("dismissed"));

    // and the rule itself can be replaced, keeping how often it has fired
    let (status, replaced) = book
        .call(
            Method::PUT,
            &format!("/rules/{rule_id}"),
            json!({
                "name": "Cash variance, renamed",
                "category": "financial",
                "severity": "low",
                "condition": {"type": "deadline", "days_before": 1},
                "updated_at": "2026-01-03T09:00:00Z"
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{replaced}");
    assert_eq!(replaced["name"], json!("Cash variance, renamed"));
    assert_eq!(replaced["created_at"], json!("2026-01-01T09:00:00Z"));

    let (status, deleted) = book
        .call(Method::DELETE, &format!("/rules/{rule_id}"), json!({}))
        .await;
    assert_eq!(status, StatusCode::OK, "{deleted}");
    let (status, _) = book.get(&format!("/rules/{rule_id}")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
