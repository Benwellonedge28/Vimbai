//! Cursor pagination: a collection answers what fits and says how to ask for the rest.
//!
//! ADR-004's sixth convention asked for `items`, `has_more` and `next_cursor` where a collection can grow without limit.
//! The engine answered a plain array and the only paging was `limit` and `offset`, which drifts: an offset counted from the
//! start of a list that a write just changed skips a record or repeats one. A cursor names the place instead of counting
//! to it.
//!
//! The convention is opt-in by request, which is what lets it land in v1 at all. A caller who sends no `cursor` gets
//! exactly the array they got before - no client breaks, no test that asserts a list changes. A caller who sends `limit`
//! or `cursor` gets the envelope. A caller who wants the new behaviour asks for it, which is the only honest way to add a
//! shape beside an existing one.
//!
//! Two things this deliberately does not do:
//!
//! * It does not re-query the store. It windows the rows the handler produced, and every list handler already caps what it
//!   reads. So `has_more` is true when the engine had more rows to give, and paging past its cap is a matter of raising that
//!   handler's cap, not of asking for a bigger page. The alternative - teaching sixty handlers to seek from a cursor - is the
//!   work of a phase, and shipping the shape now with a stated ceiling beats shipping neither.
//! * It does not sort. A handler's order is the handler's, and a cursor refers to a position in the answer the engine gave.
#![forbid(unsafe_code)]

use axum::body::{to_bytes, Body};
use axum::extract::Request;
use axum::http::{header, Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::{Json, Router};
use serde_json::{json, Value};

/// The most rows one page may hold. A caller asking for more is given this many, because an unbounded page is not a page.
pub const MAX_PAGE: usize = 500;

/// The page a caller asked for when they said nothing.
pub const DEFAULT_PAGE: usize = 50;

/// A cursor is the offset into the rows the engine produced, written so a client cannot invent one by counting.
fn encode(offset: usize) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(format!("v1:{offset}"))
}

fn decode(cursor: &str) -> Option<usize> {
    use base64::Engine as _;
    let raw = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(cursor.trim())
        .ok()?;
    let text = String::from_utf8(raw).ok()?;
    text.strip_prefix("v1:")?.parse().ok()
}

/// The query parameters this layer reads, from a raw query string.
fn parameter(query: Option<&str>, name: &str) -> Option<String> {
    let query = query?;
    for pair in query.split('&') {
        let (key, value) = match pair.split_once('=') {
            Some(pair) => pair,
            None => (pair, ""),
        };
        if key == name {
            return Some(value.replace("%20", " ").trim().to_owned());
        }
    }
    None
}

/// Put the layer in front of a router. Reads are windowed; everything else passes straight through.
pub fn guard(router: Router) -> Router {
    router.layer(axum::middleware::from_fn(
        move |request: Request, next: Next| async move { window(request, next).await },
    ))
}

async fn window(request: Request, next: Next) -> Response {
    if request.method() != Method::GET && request.method() != Method::HEAD {
        return next.run(request).await;
    }
    let query = request.uri().query().map(str::to_owned);
    let asked_limit = parameter(query.as_deref(), "limit");
    let asked_cursor = parameter(query.as_deref(), "cursor");
    if asked_limit.is_none() && asked_cursor.is_none() {
        // the shape a caller had before this layer existed, unchanged
        return next.run(request).await;
    }

    let limit = match asked_limit.as_deref() {
        None => DEFAULT_PAGE,
        Some(text) => match text.parse::<usize>() {
            Ok(number) if number > 0 => number.min(MAX_PAGE),
            _ => {
                return (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    Json(json!({"error": "limit must be a positive whole number"})),
                )
                    .into_response()
            }
        },
    };
    let offset = match asked_cursor.as_deref() {
        None => 0,
        Some(cursor) => match decode(cursor) {
            Some(offset) => offset,
            None => {
                return (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    Json(json!({"error": "this cursor is not one the engine gave out"})),
                )
                    .into_response()
            }
        },
    };

    let response = next.run(request).await;
    let status = response.status();
    let (parts, body) = response.into_parts();
    let bytes = match to_bytes(body, 8 * 1024 * 1024).await {
        Ok(bytes) => bytes,
        Err(_) => return Response::from_parts(parts, Body::empty()),
    };
    let Ok(answer) = serde_json::from_slice::<Value>(&bytes) else {
        return Response::from_parts(parts, Body::from(bytes));
    };
    // a refusal keeps the error envelope's shape: wrapping it in `items` would hide it from the caller
    if status != StatusCode::OK {
        return Response::from_parts(parts, Body::from(bytes));
    }
    // A collection reaches a client in one of two shapes: a bare array, or an object carrying the rows under a name
    // beside a count. Both are paginated the same way, and the named key keeps its name so an existing reader of it still
    // finds its rows.
    let (key, rows) = match &answer {
        Value::Array(rows) => (None, rows.clone()),
        Value::Object(fields) => {
            let named = fields
                .iter()
                .find(|(name, value)| *name != "count" && value.is_array());
            match named {
                Some((name, value)) => (
                    Some(name.clone()),
                    value.as_array().cloned().unwrap_or_default(),
                ),
                // an answer that is not a collection is not something to paginate
                None => return Response::from_parts(parts, Body::from(bytes)),
            }
        }
        _ => return Response::from_parts(parts, Body::from(bytes)),
    };

    let total = rows.len();
    let start = offset.min(total);
    let end = (start + limit).min(total);
    let page = rows[start..end].to_vec();
    let next = (end < total).then(|| encode(end));
    let mut envelope = json!({
        "items": page,
        "has_more": next.is_some(),
        "next_cursor": next,
        "count": end - start,
    });
    if let Some(name) = &key {
        // the rows keep the name the handler gave them, so a client reading `rates` still reads `rates`
        if let Some(fields) = envelope.as_object_mut() {
            fields.insert(name.clone(), Value::Array(page));
        }
    }
    if let Some(fields) = envelope.as_object_mut() {
        fields.insert("total".into(), json!(total));
    }
    // Whatever the handler said beside its rows stays said. A Book's sequence is the example that matters: it is the
    // high-water mark a device quotes back when it next pulls, and an envelope that dropped it would leave a syncing
    // client with rows it cannot place. The envelope's own names win, because those are the convention.
    if let (Some(answer), Some(envelope)) = (answer.as_object(), envelope.as_object_mut()) {
        for (name, value) in answer {
            if Some(name) == key.as_ref() || name == "count" || envelope.contains_key(name) {
                continue;
            }
            envelope.insert(name.clone(), value.clone());
        }
    }
    let body = serde_json::to_vec(&envelope).unwrap_or_default();
    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json");
    response
        .body(Body::from(body))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cursor_round_trips_and_nothing_else_does() {
        assert_eq!(decode(&encode(0)), Some(0));
        assert_eq!(decode(&encode(120)), Some(120));
        assert_eq!(decode("not-a-cursor"), None);
        assert_eq!(
            decode("djE6bm90LWEtbnVtYmVy"),
            None,
            "the version is checked, not assumed"
        );
    }

    #[test]
    fn a_query_parameter_is_read_from_a_raw_string() {
        let query = Some("limit=10&cursor=abc&name=Well%20Wisher");
        assert_eq!(parameter(query, "limit").as_deref(), Some("10"));
        assert_eq!(parameter(query, "cursor").as_deref(), Some("abc"));
        assert_eq!(parameter(query, "name").as_deref(), Some("Well Wisher"));
        assert_eq!(parameter(query, "missing"), None);
        assert_eq!(parameter(None, "limit"), None);
    }
}
