//! websocket-service over HTTP: a socket subscribes, a Book's event reaches only that Book's sockets, and the same
//! event arriving twice is carried once.

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
    path: &str,
    user: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header("x-user-id", user)
        .header("x-book-id", "book-1");
    let request = match body {
        Some(body) => {
            builder = builder.header("content-type", "application/json");
            builder.body(Body::from(body.to_string())).unwrap()
        }
        None => builder.body(Body::empty()).unwrap(),
    };
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

fn event(id: &str, book: &str) -> Value {
    json!({
        "id": id,
        "event_type": "book.resource.created",
        "source_service": "accounting-service",
        "payload": {"book_id": book, "resource_type": "journal_entry", "resource_id": "je-1",
                    "action": "created"}
    })
}

#[tokio::test]
async fn an_event_reaches_only_its_books_sockets_and_only_once() {
    let router = router();

    // two sockets on book-1's channel and one on book-2's
    for (channel, socket) in [
        ("book:book-1", "s1"),
        ("book:book-1", "s2"),
        ("book:book-2", "s3"),
    ] {
        let (status, _) = call(
            &router,
            Method::POST,
            "/realtime-subscriptions",
            "user-1",
            Some(json!({"channel": channel, "connection_id": socket})),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }
    // a subscription with no channel is refused
    let (status, _) = call(
        &router,
        Method::POST,
        "/realtime-subscriptions",
        "user-1",
        Some(json!({"connection_id": "s4"})),
    )
    .await;
    assert_ne!(status, StatusCode::OK);

    let (status, carried) = call(
        &router,
        Method::POST,
        "/events/ingest",
        "user-1",
        Some(event("evt-1", "book-1")),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{carried}");
    assert_eq!(carried["channel"], json!("book:book-1"));
    assert_eq!(carried["delivered_to"], json!(2));

    // book-2's socket is untouched: its own event reaches one
    let (_, theirs) = call(
        &router,
        Method::POST,
        "/events/ingest",
        "user-1",
        Some(event("evt-2", "book-2")),
    )
    .await;
    assert_eq!(theirs["delivered_to"], json!(1));

    // the same event again is dropped, and says nothing was delivered
    let (_, twice) = call(
        &router,
        Method::POST,
        "/events/ingest",
        "user-1",
        Some(event("evt-1", "book-1")),
    )
    .await;
    assert_eq!(twice["delivered_to"], json!(0));
    assert_eq!(twice["channel"], json!("book:book-1"));

    // an event with no Book goes nowhere rather than to a guessed channel
    let (_, nowhere) = call(
        &router,
        Method::POST,
        "/events/ingest",
        "user-1",
        Some(json!({"id": "evt-3", "payload": {}})),
    )
    .await;
    assert_eq!(nowhere, json!({"delivered_to": 0, "channel": null}));

    // a named broadcast reaches only the sockets on that channel
    let (status, sent) = call(
        &router,
        Method::POST,
        "/broadcast/book:book-1",
        "user-1",
        Some(json!({"note": "hello"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{sent}");
    assert_eq!(sent, json!({"channel": "book:book-1", "delivered_to": 2}));

    let (status, counted) = call(&router, Method::GET, "/websocket-stats", "user-1", None).await;
    assert_eq!(status, StatusCode::OK, "{counted}");
    assert_eq!(counted["total_connections"], json!(3));
    assert_eq!(counted["connections"]["book:book-1"], json!(2));
    // messages, not events: two sockets for the book-1 event, one for the book-2 event, two for the broadcast
    assert_eq!(counted["messages_sent"], json!(5));
    assert_eq!(counted["relay"]["received"], json!(2));
    assert_eq!(counted["relay"]["status"], json!("disabled"));

    let (status, healthy) = call(&router, Method::GET, "/websocket-health", "user-1", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(healthy["service"], json!("websocket-service"));

    // another person's Book sees none of it
    let (_, elsewhere) = call(&router, Method::GET, "/websocket-stats", "user-2", None).await;
    assert_eq!(elsewhere["total_connections"], json!(0));
    assert_eq!(elsewhere["messages_sent"], json!(0));
}
