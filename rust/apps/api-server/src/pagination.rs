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
//! A cursor names a row as well as a count, and that second half is what removes the ceiling this layer used to have. It
//! windows the rows a handler produced; but when the cursor carries the id of the last row handed over, the request goes on to
//! the store with that place on it, the store seeks, and the answer starts after the cursor rather than at the top. A Book
//! holding more records than a handler is willing to produce in one answer is therefore still walkable to its end: the cap
//! bounds one answer and stops bounding the walk. A handler whose listing does not go through the store answers with the top of
//! its list again, which is recognised (those rows include ids at or below the cursor) and windowed by count exactly as before.
//!
//! What it still does not do:
//!
//! * It does not sort. A windowed page keeps the handler's order; a seeked page arrives in id order, which is the only order
//!   in which a page cannot skip a record written while the previous one was being read.
//!
//! A caller who wants the page as it is produced rather than all at once asks for `application/x-ndjson` and gets one
//! record per line, written as the engine hands each one over, with the paging facts in headers (`x-total-count`,
//! `x-has-more`, `x-next-cursor`). That is what lets a client with a slow link start working before the last row exists,
//! and it keeps the engine from building one more copy of a large page in memory to send it.
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

/// A cursor is a place in a Book, not a number a client can count to.
///
/// It carries two things: how far into the answer the page ended, and the id of the last row handed over. The id is what
/// lets the store seek - `v1:{offset}` cursors handed out before this still decode, and still page, by counting alone.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Place {
    offset: usize,
    after: Option<String>,
}

fn encode(place: &Place) -> String {
    use base64::Engine as _;
    let text = match &place.after {
        Some(after) => format!("v1:{}:{after}", place.offset),
        None => format!("v1:{}", place.offset),
    };
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(text)
}

fn decode(cursor: &str) -> Option<Place> {
    use base64::Engine as _;
    let raw = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(cursor.trim())
        .ok()?;
    let text = String::from_utf8(raw).ok()?;
    let rest = text.strip_prefix("v1:")?;
    match rest.split_once(':') {
        Some((offset, after)) => Some(Place {
            offset: offset.parse().ok()?,
            after: Some(after.to_owned()).filter(|id| !id.is_empty()),
        }),
        None => Some(Place {
            offset: rest.parse().ok()?,
            after: None,
        }),
    }
}

/// The id of a row, when it has one, which is what a cursor is built from.
fn id_of(row: &Value) -> Option<&str> {
    row.get("id").and_then(Value::as_str)
}

/// The rows that come strictly after a place, in id order, and whether the answer shows it obeyed.
///
/// A handler whose listing goes through the store answers with rows past the cursor. A handler that does not hands back
/// the top of its list again, and those rows include ids at or below the cursor - which is exactly how the two are told
/// apart without every handler having to say which it is.
fn rows_after(rows: &[Value], after: &str) -> bool {
    let ids: Vec<&str> = rows.iter().filter_map(id_of).collect();
    !ids.is_empty() && ids.iter().all(|id| *id > after)
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
    let place = match asked_cursor.as_deref() {
        None => Place {
            offset: 0,
            after: None,
        },
        Some(cursor) => match decode(cursor) {
            Some(place) => place,
            None => {
                return (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    Json(json!({"error": "this cursor is not one the engine gave out"})),
                )
                    .into_response()
            }
        },
    };
    let offset = place.offset;
    let mut request = request;
    if let Some(after) = &place.after {
        if let Ok(value) = axum::http::HeaderValue::from_str(after) {
            request.headers_mut().insert("x-vimbai-after", value);
        }
    }

    let lines_wanted = request
        .headers()
        .get(header::ACCEPT)
        .and_then(|value| value.to_str().ok())
        .map(|accept| accept.to_ascii_lowercase().contains("ndjson"))
        .unwrap_or(false);
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

    // Two ways a page can be produced, and the store decides which one a caller gets.
    //
    // When the cursor names a row and the handler answered with rows past it, the store seeked: the handler's own cap no
    // longer bounds the walk, so page five hundred is one statement away instead of unreachable. Otherwise the answer is
    // the top of the list again and it is windowed by count, which is what this layer did before it could seek, and what a
    // handler that does not list through the store still gets.
    let seeked = place
        .after
        .as_deref()
        .map(|after| rows_after(&rows, after))
        .unwrap_or(false);
    let (page, taken, total) = if seeked {
        let taken = rows.len().min(limit);
        let page = rows[..taken].to_vec();
        (page, taken, rows.len())
    } else {
        let total = rows.len();
        let start = offset.min(total);
        let end = (start + limit).min(total);
        (rows[start..end].to_vec(), end - start, total)
    };
    let last = page.last().and_then(id_of).map(str::to_owned);
    // A windowed answer knows whether more follow, because it is counting rows it holds. A seeked one only knows what the
    // handler was willing to produce: a full page may be the end, so the next request answers empty and the client stops.
    // Erring that way costs one empty answer; erring the other way would silently hide rows.
    let more = if seeked {
        rows.len() >= limit
    } else {
        offset + taken < total
    };
    let next = more.then(|| {
        encode(&Place {
            offset: offset + taken,
            after: last.clone(),
        })
    });
    let mut envelope = json!({
        "items": page,
        "has_more": next.is_some(),
        "next_cursor": next,
        "count": taken,
    });
    if let Some(name) = &key {
        // the rows keep the name the handler gave them, so a client reading `rates` still reads `rates`
        if let Some(fields) = envelope.as_object_mut() {
            fields.insert(name.clone(), Value::Array(page.clone()));
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
    if lines_wanted {
        return streamed(page, &key, total, next);
    }
    let body = serde_json::to_vec(&envelope).unwrap_or_default();
    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json");
    response
        .body(Body::from(body))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

/// The page as newline-delimited JSON: one record per line, each written as the engine hands it over.
///
/// The paging facts move to headers because a line has to stay a record. A client reads them off the response and asks for
/// the next page with the cursor it was given, exactly as it would from the envelope.
fn streamed(
    page: Vec<Value>,
    key: &Option<String>,
    total: usize,
    next: Option<String>,
) -> Response {
    use futures_util::stream;
    let lines: Vec<Result<axum::body::Bytes, std::convert::Infallible>> = page
        .iter()
        .map(|row| {
            let mut text = serde_json::to_string(row).unwrap_or_else(|_| "null".to_owned());
            text.push('\n');
            Ok(axum::body::Bytes::from(text))
        })
        .collect();
    let count = lines.len();
    let mut builder = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/x-ndjson")
        .header("x-total-count", total.to_string())
        .header("x-page-count", count.to_string())
        .header("x-has-more", next.is_some().to_string());
    if let Some(name) = key {
        builder = builder.header("x-collection", name.clone());
    }
    if let Some(cursor) = next {
        builder = builder.header("x-next-cursor", cursor);
    }
    builder
        .body(Body::from_stream(stream::iter(lines)))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cursor_round_trips_and_nothing_else_does() {
        let top = Place {
            offset: 0,
            after: None,
        };
        assert_eq!(decode(&encode(&top)), Some(top));
        let deep = Place {
            offset: 120,
            after: Some("rule:42".into()),
        };
        assert_eq!(decode(&encode(&deep)), Some(deep));
        assert_eq!(decode("not-a-cursor"), None);
        assert_eq!(
            decode("djE6bm90LWEtbnVtYmVy"),
            None,
            "the version is checked, not assumed"
        );
    }

    #[test]
    fn a_cursor_from_before_the_seek_still_pages_by_count() {
        // a client holding one of these must not be broken by the layer learning to seek
        assert_eq!(
            decode("djE6NDA"),
            Some(Place {
                offset: 40,
                after: None
            })
        );
    }

    #[test]
    fn an_answer_past_the_cursor_is_told_from_the_top_of_the_list() {
        let page = vec![json!({"id": "rule:11"}), json!({"id": "rule:12"})];
        assert!(
            rows_after(&page, "rule:10"),
            "these rows come after rule:10"
        );
        assert!(
            !rows_after(&page, "rule:40"),
            "the same rows are the top of the list when the cursor is further on"
        );
        assert!(!rows_after(&[], "rule:10"), "an empty answer seeks nothing");
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
