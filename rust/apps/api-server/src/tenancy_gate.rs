//! The tenancy gate: a book-scoped request has to name a Book the caller may open.
//!
//! In the cluster the Go gateway makes this check. Nothing makes it on a machine that has no gateway, and nothing made it
//! inside the engine at all, which left the rule living in one deployment instead of in the code. This layer is that
//! rule, applied where every deployment of the engine already passes: [`crate::router`], which the binary, the on-device
//! runtime and the tests all serve.
//!
//! It is deliberately narrow. It asks the question only when a request names a Book, it takes the caller from the
//! identity the gateway injected rather than from the body, and it refuses with 403 rather than answering an empty
//! listing - a caller reaching into somebody else's organisation must be told no, not shown nothing.
#![forbid(unsafe_code)]

use axum::extract::Request;
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde_json::json;
use store::StoreError;

use crate::records::GraphHandle;

/// Addresses that answer before a Book is known. A caller cannot say which Book they mean until they have a token, so
/// the identity door has to open without one, and the membership question itself is what this gate would be asking.
const BOOK_LESS: [&str; 4] = ["/health", "/books", "/identity", "/ws"];

/// Put the gate in front of a router.
///
/// The graph is carried in the closure rather than in middleware state, so the layer needs nothing from the router it
/// wraps: this works on a router that has already had its own state applied, which is what the record routes are.
pub fn guard(router: Router, graph: GraphHandle) -> Router {
    router.layer(axum::middleware::from_fn(
        move |request: Request, next: Next| {
            let graph = graph.clone();
            async move { check(graph, request, next).await }
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

pub async fn check(graph: GraphHandle, request: Request, next: Next) -> Response {
    let path = request.uri().path().to_string();
    if BOOK_LESS.iter().any(|open| path.starts_with(open)) {
        return next.run(request).await;
    }
    let (Some(user_id), Some(book_id)) =
        (header(&request, "x-user-id"), header(&request, "x-book-id"))
    else {
        // not a book-scoped call: the routes that need a Book refuse that themselves, with their own message
        return next.run(request).await;
    };

    match tenancy::access::allows(graph.as_ref(), &user_id, &book_id) {
        Ok(_) => next.run(request).await,
        // a Book nobody has is the route's own 404 to give, not a refusal to make up here
        Err(StoreError::NotFound) => next.run(request).await,
        Err(refusal) => (
            StatusCode::FORBIDDEN,
            Json(json!({ "detail": refusal.to_string(), "error": refusal.to_string(), "book_id": book_id })),
        )
            .into_response(),
    }
}

/// A door that says which Book a caller may open, for the gateway's membership check.
///
/// It answers the same question the gate asks, through the same rule, so the two cannot disagree.
pub fn book_door(graph: GraphHandle) -> Router {
    Router::new()
        .route(
            "/book-membership",
            get(|request: Request| async move { membership_answer(graph, request).await }),
        )
        .fallback(|| async { StatusCode::NOT_FOUND })
}

async fn membership_answer(graph: GraphHandle, request: Request) -> Response {
    let (Some(user_id), Some(book_id)) =
        (header(&request, "x-user-id"), header(&request, "x-book-id"))
    else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "detail": "a membership check names a user and a book" })),
        )
            .into_response();
    };
    match tenancy::access::membership_check(graph.as_ref(), &user_id, &book_id) {
        Ok(answer) => Json(answer).into_response(),
        Err(StoreError::NotFound) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "detail": "Book not found" })),
        )
            .into_response(),
        Err(refusal) => (
            StatusCode::FORBIDDEN,
            Json(json!({ "detail": refusal.to_string() })),
        )
            .into_response(),
    }
}
