//! message-bus-service over HTTP: publishing an event, who is listening, and what every delivery attempt did.

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
async fn an_event_is_published_heard_and_audited() {
    let router = router();

    // a subscriber listening for journal entries at an address this build cannot reach
    let (status, made) = call(
        &router,
        Method::POST,
        "/bus/event-subscriptions",
        json!({"name": "ledger watcher", "event_types": ["accounting.journal_entry.created"],
               "callback_url": "https://example.test/hook", "secret": "shh"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{made}");
    let subscription_id = made["id"].as_str().expect("an id").to_string();

    // a subscription on no type at all is refused, and so is a type the bus cannot carry
    let (status, _) = call(
        &router,
        Method::POST,
        "/bus/event-subscriptions",
        json!({"name": "nobody", "event_types": [], "callback_url": "https://example.test/x"}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let (status, _) = call(
        &router,
        Method::POST,
        "/bus/event-subscriptions",
        json!({"name": "odd", "event_types": ["made.up"], "callback_url": "https://example.test/x"}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    // publishing that event: stored, fanned out, and the failure recorded rather than hidden
    let (status, published) = call(
        &router,
        Method::POST,
        "/bus/events/publish",
        json!({"type": "accounting.journal_entry.created", "source_service": "accounting-service",
               "payload": {"entry_id": "je-1", "amount": 12.5}, "timestamp": 1000.5}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{published}");
    assert_eq!(published["status"], json!("published"));
    assert_eq!(published["dispatch"]["failed"], json!(1));
    assert_eq!(published["dispatch"]["delivered"], json!(0));
    let event_id = published["event_id"].as_str().expect("an id").to_string();

    // the event is on the bus, with the attempt against it
    let (status, listed) = call(&router, Method::GET, "/bus/events", json!({})).await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    assert_eq!(listed.as_array().map(Vec::len), Some(1));
    let (_, attempts) = call(
        &router,
        Method::GET,
        &format!("/bus/events/{event_id}/deliveries"),
        json!({}),
    )
    .await;
    let rows = attempts.as_array().expect("deliveries");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["subscription_id"], json!(subscription_id));
    assert_eq!(rows[0]["status"], json!("failed"));
    assert!(rows[0]["error"].as_str().unwrap_or("").contains("TLS"));

    // an unknown type in the path is refused by name
    let (status, _) = call(
        &router,
        Method::POST,
        "/bus/events/made.up/publish",
        json!({"source_service": "a", "payload": {}}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    // the convenience door publishes by type, and the triggers raise priority on the shipped thresholds
    let (status, by_type) = call(
        &router,
        Method::POST,
        "/bus/events/finance.budget.created/publish",
        json!({"source_service": "finance-service", "payload": {"budget_id": "b-1"}}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{by_type}");
    let (_, loud) = call(
        &router,
        Method::POST,
        "/bus/trigger/budget-variance-alert",
        json!({"budget_id": "b-2", "variance_percent": 25.0, "category": "travel"}),
    )
    .await;
    let loud_id = loud["event_id"].as_str().expect("an id").to_string();
    let (_, quiet) = call(
        &router,
        Method::POST,
        "/bus/trigger/budget-variance-alert",
        json!({"budget_id": "b-3", "variance_percent": 5.0, "category": "travel"}),
    )
    .await;
    let quiet_id = quiet["event_id"].as_str().expect("an id").to_string();
    let (_, big) = call(
        &router,
        Method::GET,
        &format!("/bus/events/{loud_id}"),
        json!({}),
    )
    .await;
    let (_, small) = call(
        &router,
        Method::GET,
        &format!("/bus/events/{quiet_id}"),
        json!({}),
    )
    .await;
    assert_eq!(big["priority"], json!("high"));
    assert_eq!(small["priority"], json!("normal"));

    // the metrics count what this Book published, by type, and the types are listed
    let (_, counted) = call(&router, Method::GET, "/bus/metrics", json!({})).await;
    assert_eq!(counted["total_events"], json!(4));
    assert_eq!(
        counted["event_counts_by_type"]["finance.budget.variance_alert"],
        json!(2)
    );
    assert_eq!(counted["total_subscriptions"], json!(1));
    let (_, types) = call(&router, Method::GET, "/event-types", json!({})).await;
    assert!(types.as_array().map(Vec::len).unwrap_or(0) > 20);
    let (_, health) = call(&router, Method::GET, "/bus/health", json!({})).await;
    assert_eq!(health["events_stored"], json!(4));

    // a filter narrows the list, and deleting the subscription stops the telling
    let (_, only) = call(
        &router,
        Method::GET,
        "/bus/events?event_type=fraud.transaction.flagged",
        json!({}),
    )
    .await;
    assert_eq!(only.as_array().map(Vec::len), Some(0));
    let (status, gone) = call(
        &router,
        Method::DELETE,
        &format!("/bus/event-subscriptions/{subscription_id}"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{gone}");
    assert_eq!(gone["status"], json!("deleted"));
    let (status, _) = call(
        &router,
        Method::GET,
        &format!("/bus/event-subscriptions/{subscription_id}"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
