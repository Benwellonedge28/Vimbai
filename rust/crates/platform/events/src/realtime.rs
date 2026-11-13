#![forbid(unsafe_code)]
//! websocket-service: the last mile of the Book event stream.
//!
//! The bus already publishes `book.resource.*` for every successful write inside a Book. This service carries those
//! events the last mile: the door that takes one delivery, works out which Book it belongs to, and hands it to the
//! sockets held for that Book - and it does so ONCE, because the same event can arrive twice (over the broker and over
//! HTTP) and a client must not see it twice.
//!
//! What is ported is the door and the bookkeeping, not the sockets themselves. A socket is a live connection held by a
//! process; this service answers requests. The platform's socket layer holds the connections and registers them as
//! subscriptions here, which is what the counts are read from. Nothing registers any today, so the counts read zero and
//! `delivered_to` answers zero - honestly, rather than an invented figure - until that layer writes them. Everything a
//! client would see is decided here: the channel, the shape of the message, and whether a copy is a duplicate.
//!
//! Behaviours kept as shipped, each pinned by a test:
//!
//! * a Book is read from the payload first, then the event itself, then the envelope's metadata - and an event with no
//!   Book is DROPPED rather than broadcast to a guessed channel, answering `{"delivered_to": 0, "channel": null}`. A
//!   personal event must never leak into another tenant's stream;
//! * the same event delivered twice is carried once: an id already seen is a duplicate and answers nothing delivered.
//!   What was seen is bounded, and it is the least recently seen that goes first, so a busy stream cannot grow it
//!   without limit;
//! * the message a client would see is shaped once, here: its kind, its event id and type, its Book, the resource
//!   touched and what was done to it, its source service, its moment and the payload itself;
//! * a broadcast names a channel outright and is counted whether or not anything is listening;
//! * `messages_sent` counts MESSAGES, not events - one per listening socket - and lives on the store rather than in one
//!   process's memory, so the figure a replica reports is the Book's own rather than its share of it;
//! * the relay reads as disabled: this build holds no broker client, and saying `consuming` when nothing is consuming
//!   would be the one lie the statistics could tell.

use std::collections::BTreeMap;

use serde_json::{json, Value};

use vimbai_common::clock;
use vimbai_store::read::text;
use vimbai_store::{BookScope, Graph, RecordKind, Records, StoreError};

/// One socket listening on one channel. The platform's socket layer writes these; nothing else should.
pub const SUBSCRIPTION: RecordKind =
    RecordKind::new("RealtimeSubscription", "OWNS_REALTIME_SUBSCRIPTION");

/// One event carried to a Book's sockets.
pub const DELIVERY: RecordKind = RecordKind::new("RealtimeDelivery", "OWNS_REALTIME_DELIVERY");

/// One message broadcast to a channel by name.
pub const BROADCAST: RecordKind = RecordKind::new("RealtimeBroadcast", "OWNS_REALTIME_BROADCAST");

/// The dedupe window the shipped service used unless the deployment said otherwise.
pub const DEDUPE_WINDOW: usize = 1000;

/// The channel a Book's events belong on.
pub fn book_channel(event: &Value) -> Option<String> {
    let payload = event.get("payload").unwrap_or(&Value::Null);
    let book = payload
        .get("book_id")
        .and_then(Value::as_str)
        .or_else(|| event.get("book_id").and_then(Value::as_str))
        .or_else(|| {
            event
                .get("metadata")
                .and_then(|metadata| metadata.get("book_id"))
                .and_then(Value::as_str)
        })
        .filter(|book| !book.is_empty())?;
    Some(format!("book:{book}"))
}

/// The message a client sees: its kind, what happened, to what, in which Book, and when.
pub fn normalise(event: &Value) -> Value {
    let payload = event.get("payload").unwrap_or(&Value::Null);
    let book = payload
        .get("book_id")
        .cloned()
        .or_else(|| event.get("book_id").cloned())
        .unwrap_or(Value::Null);
    json!({
        "type": "event",
        "event_id": event.get("id").cloned().or_else(|| event.get("event_id").cloned()).unwrap_or(Value::Null),
        "event_type": event.get("event_type").cloned().or_else(|| event.get("type").cloned()).unwrap_or(Value::Null),
        "book_id": book,
        "resource_type": payload
            .get("resource_type")
            .cloned()
            .or_else(|| payload.get("resource").cloned())
            .unwrap_or(Value::Null),
        "resource_id": payload
            .get("resource_id")
            .cloned()
            .or_else(|| payload.get("id").cloned())
            .unwrap_or(Value::Null),
        "action": payload.get("action").cloned().unwrap_or(Value::Null),
        "source_service": event.get("source_service").cloned().unwrap_or(Value::Null),
        "timestamp": event.get("timestamp").cloned().unwrap_or(Value::Null),
        "payload": payload.clone()
    })
}

/// Whether an event has been carried before. The window is bounded, and the least recently seen is forgotten first.
fn already_carried(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    id: &str,
    window: usize,
) -> Result<bool, StoreError> {
    let deliveries = Records::new(graph, DELIVERY);
    let held = deliveries.list(scope, 1000)?;
    if let Some(seen) = held
        .iter()
        .find(|held| held.get("event_id").and_then(Value::as_str) == Some(id))
    {
        let seen_id = seen
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        // touching it keeps a repeated event from ageing out while its copy is still arriving
        deliveries.update(scope, &seen_id, &json!({"seen_at": clock::moment()}))?;
        return Ok(true);
    }
    // let the oldest go once the window is full
    if held.len() >= window.max(1) {
        if let Some(oldest) = held
            .iter()
            .min_by(|one, other| {
                let at = |held: &Value| held.get("seen_at").and_then(Value::as_f64).unwrap_or(0.0);
                at(one)
                    .partial_cmp(&at(other))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .and_then(|held| held.get("id").and_then(Value::as_str).map(str::to_owned))
        {
            deliveries.delete(scope, &oldest)?;
        }
    }
    Ok(false)
}

/// How many sockets are listening on one channel.
fn listeners(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    channel: &str,
) -> Result<i64, StoreError> {
    Ok(Records::new(graph, SUBSCRIPTION)
        .list(scope, 1000)?
        .iter()
        .filter(|held| held.get("channel").and_then(Value::as_str) == Some(channel))
        .count() as i64)
}

/// Carry one bus event to the sockets of its own Book, answering how many were written to and on which channel.
pub fn ingest(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    event: &Value,
    window: usize,
) -> Result<Value, StoreError> {
    let Some(channel) = book_channel(event) else {
        // no Book, no channel: an event with nowhere safe to go is dropped rather than guessed at
        return Ok(json!({"delivered_to": 0, "channel": Value::Null}));
    };
    let id = event
        .get("id")
        .or_else(|| event.get("event_id"))
        .and_then(Value::as_str);
    if let Some(id) = id {
        if already_carried(graph, scope, id, window)? {
            return Ok(json!({"delivered_to": 0, "channel": channel}));
        }
    }

    let mut message = normalise(event);
    if message["timestamp"].is_null() {
        // a client is shown a moment; an event that stated none is stamped rather than shown an empty field
        message["timestamp"] = json!(clock::moment());
    }
    // the Book is the scope's, never a field of its own: the store stamps it, and a record that named it again could
    // disagree with the scope it was written under
    let reached = listeners(graph, scope, &channel)?;
    Records::new(graph, DELIVERY).create(
        scope,
        &json!({
            "channel": channel,
            "event_id": id,
            "event_type": message["event_type"].clone(),
            "resource_type": message["resource_type"].clone(),
            "resource_id": message["resource_id"].clone(),
            "action": message["action"].clone(),
            "source_service": message["source_service"].clone(),
            "delivered_to": reached,
            "seen_at": clock::moment(),
            "message": message,
        }),
    )?;
    Ok(json!({"delivered_to": reached, "channel": channel}))
}

/// Name a channel outright and broadcast to it.
pub fn broadcast(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    channel: &str,
    message: &Value,
) -> Result<Value, StoreError> {
    if channel.trim().is_empty() {
        return Err(StoreError::Invalid("a channel is required".into()));
    }
    let reached = listeners(graph, scope, channel)?;
    Records::new(graph, BROADCAST).create(
        scope,
        &json!({
            "channel": channel,
            "type": "broadcast",
            "data": message,
            "delivered_to": reached,
            "timestamp": clock::moment(),
        }),
    )?;
    Ok(json!({"channel": channel, "delivered_to": reached}))
}

/// The sockets listening on one channel, counted.
pub fn channel_stats(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
) -> Result<Value, StoreError> {
    let mut channels: BTreeMap<String, i64> = BTreeMap::new();
    for held in Records::new(graph, SUBSCRIPTION).list(scope, 1000)? {
        let channel = text(&held, "channel");
        if channel.is_empty() {
            continue;
        }
        *channels.entry(channel).or_insert(0) += 1;
    }

    // messages, not events: one per listening socket at the moment it was carried
    let deliveries = Records::new(graph, DELIVERY).list(scope, 1000)?;
    let mut sent: i64 = deliveries
        .iter()
        .map(|held| {
            held.get("delivered_to")
                .and_then(Value::as_i64)
                .unwrap_or(0)
        })
        .sum();
    for held in Records::new(graph, BROADCAST).list(scope, 1000)? {
        sent += held
            .get("delivered_to")
            .and_then(Value::as_i64)
            .unwrap_or(0);
    }

    Ok(json!({
        "connections": channels,
        "total_connections": channels.values().sum::<i64>(),
        "messages_sent": sent,
        "relay": {
            // this build holds no broker client, so nothing is consuming and saying otherwise would be a lie
            "status": "disabled",
            "received": deliveries.len(),
            "last_error": Value::Null
        }
    }))
}

/// The health an operator asks for: what is on, and how many channels have sockets.
pub fn health(graph: &(impl Graph + ?Sized), scope: &BookScope) -> Result<Value, StoreError> {
    let counted = channel_stats(graph, scope)?;
    Ok(json!({
        "status": "healthy",
        "service": "websocket-service",
        "active_channels": counted.get("connections").cloned().unwrap_or(json!({}))
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::MemoryGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn event(id: &str, book: Option<&str>) -> Value {
        let payload = match book {
            Some(book) => {
                json!({"book_id": book, "resource_type": "journal_entry", "resource_id": "je-1",
                                 "action": "created"})
            }
            None => json!({}),
        };
        json!({
            "id": id,
            "event_type": "book.resource.created",
            "source_service": "accounting-service",
            "timestamp": "2026-11-13T10:00:00Z",
            "payload": payload
        })
    }

    fn listening(graph: &MemoryGraph, channel: &str, who: &str) {
        Records::new(graph, SUBSCRIPTION)
            .create(
                &scope(),
                &json!({"channel": channel, "connection_id": who, "opened_at": clock::moment()}),
            )
            .expect("a socket");
    }

    #[test]
    fn a_book_is_read_from_the_payload_then_the_event_then_its_metadata() {
        assert_eq!(
            book_channel(&event("e", Some("book-9"))).as_deref(),
            Some("book:book-9")
        );
        assert_eq!(
            book_channel(&json!({"payload": {}, "book_id": "book-3"})).as_deref(),
            Some("book:book-3")
        );
        assert_eq!(
            book_channel(&json!({"payload": {}, "metadata": {"book_id": "book-7"}})).as_deref(),
            Some("book:book-7")
        );
        // an event with no Book belongs on no channel at all
        assert_eq!(book_channel(&json!({"payload": {}})), None);
        assert_eq!(book_channel(&json!({"payload": {"book_id": ""}})), None);
    }

    #[test]
    fn the_message_a_client_sees_is_shaped_once() {
        let shaped = normalise(&event("evt-1", Some("book-1")));
        assert_eq!(shaped["type"], json!("event"));
        assert_eq!(shaped["event_id"], json!("evt-1"));
        assert_eq!(shaped["event_type"], json!("book.resource.created"));
        assert_eq!(shaped["book_id"], json!("book-1"));
        assert_eq!(shaped["resource_type"], json!("journal_entry"));
        assert_eq!(shaped["resource_id"], json!("je-1"));
        assert_eq!(shaped["action"], json!("created"));
        assert_eq!(shaped["source_service"], json!("accounting-service"));
        assert_eq!(shaped["timestamp"], json!("2026-11-13T10:00:00Z"));
    }

    #[test]
    fn only_the_books_own_listeners_are_reached() {
        let graph = MemoryGraph::new();
        listening(&graph, "book:book-1", "socket-1");
        listening(&graph, "book:book-2", "socket-2");

        let carried = ingest(
            &graph,
            &scope(),
            &event("evt-1", Some("book-1")),
            DEDUPE_WINDOW,
        )
        .expect("carried");
        assert_eq!(carried["channel"], json!("book:book-1"));
        assert_eq!(carried["delivered_to"], json!(1));

        // an event for the other Book reaches only that one's socket
        let theirs = ingest(
            &graph,
            &scope(),
            &event("evt-2", Some("book-2")),
            DEDUPE_WINDOW,
        )
        .expect("carried");
        assert_eq!(theirs["delivered_to"], json!(1));
        // a Book nobody listens on answers zero rather than failing
        let none = ingest(
            &graph,
            &scope(),
            &event("evt-3", Some("book-9")),
            DEDUPE_WINDOW,
        )
        .expect("carried");
        assert_eq!(none["delivered_to"], json!(0));
    }

    #[test]
    fn the_same_event_delivered_twice_is_carried_once() {
        let graph = MemoryGraph::new();
        listening(&graph, "book:book-1", "socket-1");
        let once = ingest(
            &graph,
            &scope(),
            &event("dup", Some("book-1")),
            DEDUPE_WINDOW,
        )
        .expect("carried");
        assert_eq!(once["delivered_to"], json!(1));
        // the second copy is dropped, and says so on the channel it would have used
        let twice = ingest(
            &graph,
            &scope(),
            &event("dup", Some("book-1")),
            DEDUPE_WINDOW,
        )
        .expect("carried");
        assert_eq!(twice["delivered_to"], json!(0));
        assert_eq!(twice["channel"], json!("book:book-1"));

        // one delivery is recorded, not two
        assert_eq!(
            Records::new(&graph, DELIVERY)
                .list(&scope(), 100)
                .expect("held")
                .len(),
            1
        );
    }

    #[test]
    fn the_window_of_what_was_seen_is_bounded() {
        let graph = MemoryGraph::new();
        // a window of two: the third event forgets the first, which may then be carried again
        for id in ["one", "two", "three"] {
            ingest(&graph, &scope(), &event(id, Some("book-1")), 2).expect("carried");
        }
        assert_eq!(
            Records::new(&graph, DELIVERY)
                .list(&scope(), 100)
                .expect("held")
                .len(),
            2
        );
        let again = ingest(&graph, &scope(), &event("one", Some("book-1")), 2).expect("carried");
        assert_eq!(again["channel"], json!("book:book-1"));
        assert_eq!(
            Records::new(&graph, DELIVERY)
                .list(&scope(), 100)
                .expect("held")
                .len(),
            2
        );
    }

    #[test]
    fn an_event_with_no_book_is_dropped_rather_than_guessed_at() {
        let graph = MemoryGraph::new();
        listening(&graph, "book:book-1", "socket-1");
        let dropped =
            ingest(&graph, &scope(), &event("evt-1", None), DEDUPE_WINDOW).expect("dropped");
        assert_eq!(dropped, json!({"delivered_to": 0, "channel": Value::Null}));
        // and nothing was recorded, so a later copy is not treated as a duplicate
        assert!(Records::new(&graph, DELIVERY)
            .list(&scope(), 100)
            .expect("held")
            .is_empty());
    }

    #[test]
    fn a_broadcast_is_counted_and_the_statistics_agree() {
        let graph = MemoryGraph::new();
        listening(&graph, "book:book-1", "socket-1");
        listening(&graph, "book:book-1", "socket-2");
        listening(&graph, "alerts", "socket-3");

        let carried = ingest(
            &graph,
            &scope(),
            &event("evt-1", Some("book-1")),
            DEDUPE_WINDOW,
        )
        .expect("carried");
        assert_eq!(carried["delivered_to"], json!(2));
        let sent =
            broadcast(&graph, &scope(), "alerts", &json!({"note": "hello"})).expect("broadcast");
        assert_eq!(sent, json!({"channel": "alerts", "delivered_to": 1}));
        // a channel with nothing listening is still a broadcast, and answers zero
        let empty = broadcast(&graph, &scope(), "quiet", &json!({"note": "x"})).expect("broadcast");
        assert_eq!(empty["delivered_to"], json!(0));
        assert!(broadcast(&graph, &scope(), "  ", &json!({})).is_err());

        let counted = channel_stats(&graph, &scope()).expect("counted");
        assert_eq!(counted["total_connections"], json!(3));
        assert_eq!(counted["connections"]["book:book-1"], json!(2));
        assert_eq!(counted["connections"]["alerts"], json!(1));
        // messages, not events: two sockets for the event, one socket for the alerts broadcast, and none for the quiet
        // one - three messages, where four events were counted
        assert_eq!(counted["messages_sent"], json!(3));
        assert_eq!(counted["relay"]["received"], json!(1));
        // nothing is consuming, so the relay says so rather than claiming to be
        assert_eq!(counted["relay"]["status"], json!("disabled"));
        assert!(counted["relay"]["last_error"].is_null());

        let healthy = health(&graph, &scope()).expect("healthy");
        assert_eq!(healthy["status"], json!("healthy"));
        assert_eq!(healthy["service"], json!("websocket-service"));
    }

    #[test]
    fn one_books_counts_are_not_anothers() {
        let graph = MemoryGraph::new();
        listening(&graph, "book:book-1", "socket-1");
        ingest(
            &graph,
            &scope(),
            &event("evt-1", Some("book-1")),
            DEDUPE_WINDOW,
        )
        .expect("carried");
        // a different caller in a different Book sees nothing of it
        let elsewhere =
            channel_stats(&graph, &BookScope::new("user-2", "book-2")).expect("counted");
        assert_eq!(elsewhere["total_connections"], json!(0));
        assert_eq!(elsewhere["messages_sent"], json!(0));
    }
}
