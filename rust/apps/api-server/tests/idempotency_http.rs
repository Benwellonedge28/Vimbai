#![forbid(unsafe_code)]
//! A write that a client retries happens once.
//!
//! Before this layer, ADR-004's seventh convention was a sentence and a timed-out retry posted twice. The tests here hold
//! the promises the layer makes: a repeat replays the first answer instead of doing the work again, a repeat with a
//! different body is refused rather than guessed at, a repeat while the first is still running is told to wait, and a
//! write with no key is left exactly as it was.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use axum::routing::post;
use axum::{Json, Router};
use serde_json::{json, Value};
use store::fake::MemoryGraph;
use tower::ServiceExt;

/// A handler that counts its own invocations, so "the work happened once" is measured rather than assumed.
#[derive(Clone)]
struct Counter {
    calls: Arc<AtomicUsize>,
}

impl Counter {
    fn new() -> Self {
        Self {
            calls: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }

    fn router(&self, graph: Arc<MemoryGraph>) -> Router {
        let counter = self.clone();
        let probe = Router::new().route(
            "/probe",
            post(move |Json(body): Json<Value>| {
                let counter = counter.clone();
                async move {
                    counter.calls.fetch_add(1, Ordering::SeqCst);
                    (StatusCode::CREATED, Json(json!({ "made": body["name"] })))
                }
            }),
        );
        let state: vimbai_api::records::GraphHandle = graph;
        vimbai_api::idempotency::guard(probe.with_state(()), state)
    }
}

async fn send(
    router: Router,
    method: Method,
    uri: &str,
    key: Option<&str>,
    body: Value,
) -> (StatusCode, Value, bool) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .header("x-user-id", "user-1")
        .header("x-book-id", "book-1");
    if let Some(key) = key {
        builder = builder.header("idempotency-key", key);
    }
    let response = router
        .oneshot(builder.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let replay = response
        .headers()
        .get("idempotent-replay")
        .is_some_and(|value| value == "true");
    let bytes = axum::body::to_bytes(response.into_body(), 1_000_000)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        replay,
    )
}

#[tokio::test]
async fn a_repeat_replays_the_first_answer_instead_of_doing_the_work_again() {
    let counter = Counter::new();
    let graph = Arc::new(MemoryGraph::default());
    let first = send(
        counter.router(graph.clone()),
        Method::POST,
        "/probe",
        Some("k-1"),
        json!({"name": "rent"}),
    )
    .await;
    assert_eq!(first.0, StatusCode::CREATED);
    assert_eq!(first.1, json!({"made": "rent"}));
    assert!(!first.2, "the first answer is not a replay");

    let second = send(
        counter.router(graph.clone()),
        Method::POST,
        "/probe",
        Some("k-1"),
        json!({"name": "rent"}),
    )
    .await;
    assert_eq!(second.0, StatusCode::CREATED);
    assert_eq!(
        second.1, first.1,
        "the replay is the first answer, not a new one"
    );
    assert!(second.2, "the second answer says it is a replay");
    assert_eq!(counter.calls(), 1, "the handler ran once, not twice");

    // and a third, because a retry is often a third
    let third = send(
        counter.router(graph),
        Method::POST,
        "/probe",
        Some("k-1"),
        json!({"name": "rent"}),
    )
    .await;
    assert_eq!(third.1, first.1);
    assert_eq!(counter.calls(), 1);
}

#[tokio::test]
async fn the_same_key_on_a_different_request_is_refused_rather_than_guessed_at() {
    let counter = Counter::new();
    let graph = Arc::new(MemoryGraph::default());
    let _ = send(
        counter.router(graph.clone()),
        Method::POST,
        "/probe",
        Some("k-2"),
        json!({"name": "rent"}),
    )
    .await;
    let (status, body, _) = send(
        counter.router(graph),
        Method::POST,
        "/probe",
        Some("k-2"),
        json!({"name": "wages"}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("different request"),
        "{body}"
    );
    assert_eq!(counter.calls(), 1, "the second request did not run");
}

#[tokio::test]
async fn a_different_key_does_the_work_again() {
    let counter = Counter::new();
    let graph = Arc::new(MemoryGraph::default());
    let _ = send(
        counter.router(graph.clone()),
        Method::POST,
        "/probe",
        Some("k-3"),
        json!({"name": "rent"}),
    )
    .await;
    let (status, _, replay) = send(
        counter.router(graph),
        Method::POST,
        "/probe",
        Some("k-4"),
        json!({"name": "rent"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert!(!replay);
    assert_eq!(counter.calls(), 2, "two keys are two writes");
}

#[tokio::test]
async fn a_write_with_no_key_is_left_alone() {
    // the header is opt-in: most writes do not need a key and must not pay for one
    let counter = Counter::new();
    let graph = Arc::new(MemoryGraph::default());
    let _ = send(
        counter.router(graph.clone()),
        Method::POST,
        "/probe",
        None,
        json!({"name": "rent"}),
    )
    .await;
    let _ = send(
        counter.router(graph),
        Method::POST,
        "/probe",
        None,
        json!({"name": "rent"}),
    )
    .await;
    assert_eq!(counter.calls(), 2);
}

#[tokio::test]
async fn a_read_is_never_held_back_by_a_key() {
    let graph = Arc::new(MemoryGraph::default());
    let counter = Counter::new();
    let state: vimbai_api::records::GraphHandle = graph;
    let probe = Router::new().route(
        "/probe",
        axum::routing::get(|| async { (StatusCode::OK, Json(json!({"read": true}))) }),
    );
    let router = vimbai_api::idempotency::guard(probe, state);
    assert_eq!(counter.calls(), 0);
    let (status, body, _) = send(router, Method::GET, "/probe", Some("k-5"), Value::Null).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({"read": true}));
}

#[tokio::test]
async fn a_refusal_is_replayed_as_a_refusal() {
    // an idempotent refusal stays refused: the second call must not get a fresh chance to succeed
    let calls = Arc::new(AtomicUsize::new(0));
    let counted = calls.clone();
    let probe = Router::new().route(
        "/probe",
        post(move || {
            let counted = counted.clone();
            async move {
                counted.fetch_add(1, Ordering::SeqCst);
                (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    Json(json!({"error": "a zero donation is not a donation"})),
                )
            }
        }),
    );
    let state: vimbai_api::records::GraphHandle = Arc::new(MemoryGraph::default());
    let first = send(
        vimbai_api::idempotency::guard(probe.clone(), state.clone()),
        Method::POST,
        "/probe",
        Some("k-6"),
        json!({}),
    )
    .await;
    assert_eq!(first.0, StatusCode::UNPROCESSABLE_ENTITY);
    let second = send(
        vimbai_api::idempotency::guard(probe, state),
        Method::POST,
        "/probe",
        Some("k-6"),
        json!({}),
    )
    .await;
    assert_eq!(second.0, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(second.1, first.1, "the refusal is the recorded refusal");
    assert_eq!(calls.load(Ordering::SeqCst), 1, "the handler ran once");
}

#[tokio::test]
async fn a_claim_left_running_is_told_to_wait_rather_than_answered() {
    // a half-finished write is the one case where neither replaying nor running is right
    let graph = Arc::new(MemoryGraph::default());
    let state: vimbai_api::records::GraphHandle = graph.clone();
    let counter = Counter::new();
    let probe = Router::new().route(
        "/probe",
        post(|| async {
            (
                StatusCode::METHOD_NOT_ALLOWED,
                Json(json!({"error": "must not run"})),
            )
        }),
    );
    // a claim the layer did not write: what a crashed first attempt leaves behind
    let fingerprint = {
        let bytes = b"{}".to_vec();
        let mut data = b"POST\n/probe\n".to_vec();
        data.extend_from_slice(&bytes);
        common::hash::sha256_hex(&data)
    };
    let id = common::hash::sha256_hex(b"user-1\x1fbook-1\x1fk-7");
    let claims = store::PlatformRecords::new(&*graph, vimbai_api::idempotency::CLAIM);
    claims
        .create(&json!({
            "id": id,
            "caller": "user-1",
            "scope": "book-1",
            "key": "k-7",
            "fingerprint": fingerprint,
            "state": "in_progress",
        }))
        .expect("a claim");
    let router = vimbai_api::idempotency::guard(probe, state);
    let (status, body, _) = send(router, Method::POST, "/probe", Some("k-7"), json!({})).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(
        body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("still running"),
        "{body}"
    );
    assert_eq!(counter.calls(), 0);
}

#[tokio::test]
async fn the_layer_is_mounted_where_the_binaries_serve() {
    // through the assembled engine, not the layer alone: a guard nobody mounts protects nothing
    let graph: vimbai_api::records::GraphHandle = Arc::new(MemoryGraph::default());
    let router = vimbai_api::routed(Some(graph));
    let open = json!({ "name": "Moyo Traders", "currency": "USD", "tier": "business" });
    let post = |uri: String, key: String, body: Value| {
        let router = router.clone();
        async move {
            let request = Request::builder()
                .method(Method::POST)
                .uri(uri)
                .header("content-type", "application/json")
                .header("x-user-id", "user-1")
                .header("idempotency-key", key)
                .body(Body::from(body.to_string()))
                .unwrap();
            let response = router.oneshot(request).await.unwrap();
            let status = response.status();
            let bytes = axum::body::to_bytes(response.into_body(), 1_000_000)
                .await
                .unwrap();
            (
                status,
                serde_json::from_slice::<Value>(&bytes).unwrap_or(Value::Null),
            )
        }
    };
    let first = post("/books".to_string(), "book-once".to_string(), open.clone()).await;
    assert!(first.0.is_success(), "{:?}", first);
    let second = post("/books".to_string(), "book-once".to_string(), open).await;
    assert!(second.0.is_success(), "{:?}", second);
    assert_eq!(second.1, first.1, "the same Book, not a second one");
}
