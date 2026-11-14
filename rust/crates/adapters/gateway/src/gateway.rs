//! The gateway contract, for a machine that has no gateway.
//!
//! In the cluster the Go gateway verifies the caller's token, injects `X-User-Id` and refuses a Book the caller is not a
//! member of. Nothing does that on a laptop, so the engine would either trust a client-supplied header or refuse every
//! book-scoped call. This module is that missing middle: the same three rules, answered by the engine itself.
//!
//! It deliberately does not reimplement the gateway. There is no routing table here, because the engine already answers
//! every address; no membership query, because the engine already holds the Book; and no fallback identity, because a
//! caller without a token is nobody.

use axum::body::Body;
use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Router;
use chrono::Utc;
use serde_json::json;
use tower::ServiceExt;

use crate::addresses::{self, Mount};

/// Addresses a person has to reach before they can hold a token: signing in is how somebody becomes somebody.
const OPEN: [&str; 5] = ["/identity", "/health", "/docs", "/redoc", "/openapi.json"];

#[derive(Clone)]
pub struct Gateway {
    pub secret: String,
    /// The published prefixes and which of them the engine answers stripped down.
    pub mounts: Vec<Mount>,
    /// The engine's own router, so membership is asked of the thing that holds the Book rather than a second copy of it.
    pub engine: Router,
}

pub async fn check(State(gateway): State<Gateway>, mut request: Request, next: Next) -> Response {
    let path = request.uri().path().to_string();
    if path.is_empty() || path == "/" || OPEN.iter().any(|open| path.starts_with(open)) {
        return next.run(request).await;
    }

    let token = request
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(str::to_string);
    let Some(token) = token else {
        return refusal(401, "Not authenticated");
    };
    let Some(claims) = sso::accounts::verify_token(&token, &gateway.secret, Utc::now()) else {
        return refusal(401, "Invalid or expired token");
    };

    let book = request
        .headers()
        .get("x-book-id")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    if let Some(book_id) = book {
        if !is_member(&gateway.engine, &claims.subject, &book_id).await {
            return refusal(403, &format!("No active membership in book {book_id}"));
        }
    }

    // The verified identity replaces anything the client sent, so `X-User-Id` cannot be claimed.
    request.headers_mut().remove("x-user-id");
    if let Ok(value) = claims.subject.parse() {
        request.headers_mut().insert("x-user-id", value);
    }

    // The address the client was told to call, turned into the address the engine answers. This is the whole of the
    // gateway's routing table, and it comes from the manifest rather than a second copy of it.
    let asked = request.uri().path().to_string();
    let engine_address = addresses::engine_path(&gateway.mounts, &asked);
    if engine_address != asked {
        let mut parts = request.uri().clone().into_parts();
        let query = parts
            .path_and_query
            .as_ref()
            .and_then(|path_and_query| path_and_query.query())
            .map(|query| format!("?{query}"))
            .unwrap_or_default();
        if let Ok(path_and_query) = format!("{engine_address}{query}").parse() {
            parts.path_and_query = Some(path_and_query);
            if let Ok(uri) = axum::http::Uri::from_parts(parts) {
                *request.uri_mut() = uri;
            }
        }
    }
    next.run(request).await
}

/// True when the engine says this person is an active member of this Book. The engine answers 404 for no membership and
/// 403 when the caller may not look; both mean the same thing to a gate.
async fn is_member(engine: &Router, user_id: &str, book_id: &str) -> bool {
    let request = Request::builder()
        .uri(format!("/books/{book_id}/membership"))
        .header("x-user-id", user_id)
        .header("x-book-id", book_id)
        .body(Body::empty());
    let Ok(request) = request else {
        return false;
    };
    match engine.clone().oneshot(request).await {
        Ok(response) => response.status().is_success(),
        Err(_) => false,
    }
}

fn refusal(status: u16, message: &str) -> Response {
    (
        axum::http::StatusCode::from_u16(status)
            .unwrap_or(axum::http::StatusCode::INTERNAL_SERVER_ERROR),
        axum::Json(json!({ "detail": message, "error": message })),
    )
        .into_response()
}
