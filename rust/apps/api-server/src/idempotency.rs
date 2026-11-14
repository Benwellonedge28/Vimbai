//! Idempotency keys: a write that a client retries happens once.
//!
//! ADR-004 convention 7 said a write that must not repeat carries a key. Until this layer existed the convention was a
//! sentence: a client that timed out and retried posted twice, and the two entries were indistinguishable. The place it
//! matters most is a posting, and the only repair was a person reversing one of them.
//!
//! The mechanism is defined once, here, in the layer every deployment of the engine already passes - the same place the
//! tenancy gate sits, so there is no second copy to drift from it.
//!
//! How it answers:
//!
//! * A write with no `Idempotency-Key` is untouched. The header is opt-in, because a key costs a write and most writes do
//!   not need one; a caller that needs the guarantee asks for it.
//! * The first request with a key is claimed, run, and its answer recorded. The claim is written *before* the work, so a
//!   crash mid-write leaves a claim saying the work was started rather than a silent gap.
//! * A repeat with the same key and the same fingerprint replays the recorded status and body. The client gets the answer
//!   it should have got the first time, including a refusal: an idempotent refusal stays refused.
//! * A repeat with the same key and a *different* request is refused with 422. Reusing a key for a different write is a
//!   client bug, and answering either of the two would be wrong.
//! * A repeat while the first is still running answers 409 and says so. It is not replayed as success, because the work
//!   has not finished, and it is not run, because that is the duplicate this exists to prevent.
//!
//! The fingerprint is the method, the path and a hash of the body. The claim is keyed by the caller and the Book, so two
//! callers cannot collide on a key, and one caller's key cannot replay another's answer.
#![forbid(unsafe_code)]

use axum::body::to_bytes;
use axum::extract::Request;
use axum::http::{header, Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::{Json, Router};
use common::hash::sha256_hex;
use serde_json::{json, Value};
use store::PlatformRecords;

use crate::records::GraphHandle;

/// The platform record a claim lives in. Platform-scoped rather than Book-scoped: a claim is bookkeeping about a request,
/// not data in the Book, and a Book's records are answers a client reads.
pub const CLAIM: store::RecordKind = store::IDEMPOTENCY_CLAIM;

/// The header a caller sends, spelled the way every HTTP client spells it.
const HEADER: &str = "idempotency-key";

/// The body a claim keeps. Long enough for any answer the engine gives; a claim that truncated one would replay a lie.
const MAX_ANSWER: usize = 1_048_576;

/// Put the layer in front of a router.
pub fn guard(router: Router, graph: GraphHandle) -> Router {
    router.layer(axum::middleware::from_fn(
        move |request: Request, next: Next| {
            let graph = graph.clone();
            async move { relevant(graph, request, next).await }
        },
    ))
}

/// A header the caller sent, trimmed, or nothing when it was absent or blank.
fn header(request: &Request, name: &str) -> Option<String> {
    request
        .headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn is_write(method: &Method) -> bool {
    matches!(
        *method,
        Method::POST | Method::PUT | Method::PATCH | Method::DELETE
    )
}

/// The claim's id: the caller, the Book and the key. Hashed so a key may be any string a client likes, and so a Book-less
/// write - which has no Book to scope it to - still gets a claim of its own.
fn claim_id(user_id: &str, book_id: &str, key: &str) -> String {
    sha256_hex(format!("{user_id}\u{1f}{book_id}\u{1f}{key}").as_bytes())
}

/// What this request *is*, so a repeat with a different payload is not mistaken for a repeat.
fn fingerprint(method: &Method, path: &str, body: &[u8]) -> String {
    sha256_hex(
        &[
            method.as_str().as_bytes(),
            b"\n",
            path.as_bytes(),
            b"\n",
            body,
        ]
        .concat(),
    )
}

async fn relevant(graph: GraphHandle, request: Request, next: Next) -> Response {
    if !is_write(request.method()) || header(&request, HEADER).is_none() {
        return next.run(request).await;
    }
    // the caller is always known by the time a write arrives: the gateway set the header from the token, and a request
    // without one cannot be attributed to a key. The Book may be absent and still deserve a key - opening a Book is a
    // write that must not happen twice, and there is no Book in scope yet to scope it to.
    let Some(user_id) = header(&request, "x-user-id") else {
        return next.run(request).await;
    };
    let book_id = header(&request, "x-book-id").unwrap_or_default();
    let key = header(&request, HEADER).unwrap_or_default();
    let method = request.method().clone();
    let path = request.uri().path().to_string();

    // the body has to be read to fingerprint it, and read back into the request afterwards
    let (parts, body) = request.into_parts();
    let bytes = match to_bytes(body, MAX_ANSWER).await {
        Ok(bytes) => bytes,
        Err(_) => {
            return (
                StatusCode::PAYLOAD_TOO_LARGE,
                Json(json!({"error": "the request body is too large to record against an idempotency key"})),
            )
                .into_response()
        }
    };
    let fingerprint = fingerprint(&method, &path, &bytes);
    let id = claim_id(&user_id, &book_id, &key);

    let claims = PlatformRecords::new(&*graph, CLAIM);
    match claims.get(&id) {
        Ok(Some(claim)) => {
            let recorded = claim
                .get("fingerprint")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if recorded != fingerprint {
                return (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    Json(json!({
                        "error": "this idempotency key was used for a different request; use a new key for a new write"
                    })),
                )
                    .into_response();
            }
            let state = claim
                .get("state")
                .and_then(Value::as_str)
                .unwrap_or("in_progress");
            if state != "done" {
                return (
                    StatusCode::CONFLICT,
                    Json(json!({
                        "error": "a request with this idempotency key is still running; retry when it has finished"
                    })),
                )
                    .into_response();
            }
            let status = claim.get("status").and_then(Value::as_u64).unwrap_or(200) as u16;
            let body = claim
                .get("body")
                .and_then(Value::as_str)
                .unwrap_or_default();
            return Response::builder()
                .status(StatusCode::from_u16(status).unwrap_or(StatusCode::OK))
                .header(header::CONTENT_TYPE, "application/json")
                .header("idempotent-replay", "true")
                .body(axum::body::Body::from(body.to_owned()))
                .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response());
        }
        Ok(None) => {}
        Err(problem) => {
            // the store did not answer, so no claim can be trusted: refuse rather than risk a duplicate
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({"error": format!("the idempotency record could not be read: {problem}")})),
            )
                .into_response();
        }
    }

    let claim = json!({
        "id": id,
        "caller": user_id,
        "scope": book_id,
        "key": key,
        "fingerprint": fingerprint,
        "state": "in_progress",
        "created_at": common::clock::moment(),
    });
    if let Err(problem) = claims.create(&claim) {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(
                json!({"error": format!("the idempotency record could not be written: {problem}")}),
            ),
        )
            .into_response();
    }

    let response = next
        .run(Request::from_parts(parts, axum::body::Body::from(bytes)))
        .await;
    let status = response.status();
    let (parts, body) = response.into_parts();
    let answer = match to_bytes(body, MAX_ANSWER).await {
        Ok(answer) => answer,
        Err(_) => {
            // the work is done and its answer is too large to keep. Record the status without a body rather than claim a
            // replay that would differ, and let the caller see what happened.
            let _ = claims.update(
                &id,
                &json!({ "state": "done", "status": status.as_u16(), "replayable": false }),
            );
            return Response::from_parts(parts, axum::body::Body::empty());
        }
    };
    let text = String::from_utf8_lossy(&answer).into_owned();
    let _ = claims.update(
        &id,
        &json!({ "state": "done", "status": status.as_u16(), "body": text }),
    );
    Response::from_parts(parts, axum::body::Body::from(answer))
}

/// How long a claim keeps its promise, in seconds.
///
/// A client retries within a minute or two; a day is generous by three orders of magnitude and still bounded. The worker
/// removes the claims that outlive this; see `docs/WORKER.md`.
pub const CLAIM_LIFETIME_SECONDS: f64 = 86_400.0;
