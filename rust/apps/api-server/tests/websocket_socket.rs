#![forbid(unsafe_code)]
//! The socket door, end to end: a real client connects over a real socket, is handed the handshake, receives the event
//! carried for its Book, has its own message carried to the rest of the channel, and is told when another client goes.
//!
//! A cheaper test would call the handler and inspect a channel, but the thing being proven here is the thing that was
//! missing from the port: an actual connection held open, and written to. So this test speaks the protocol.

use std::sync::Arc;

use events::sockets::hub;
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use store::fake::MemoryGraph;
use tokio::net::TcpListener;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

/// Serve the record routes on a free port and answer where to reach them.
async fn serve(graph: Arc<MemoryGraph>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("a port");
    let address = listener.local_addr().expect("an address");
    tokio::spawn(async move {
        axum::serve(listener, vimbai_api::records::router(graph))
            .await
            .expect("served");
    });
    format!("127.0.0.1:{}", address.port())
}

fn an_event(id: &str, book: &str) -> Value {
    json!({
        "id": id,
        "event_type": "book.resource.created",
        "source_service": "accounting-service",
        "timestamp": "2026-11-14T09:00:00Z",
        "payload": {"book_id": book, "resource_type": "journal_entry", "resource_id": "je-1",
                    "action": "created"}
    })
}

async fn next_text(
    socket: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
) -> Value {
    let frame = socket.next().await.expect("a frame").expect("a frame");
    match frame {
        Message::Text(text) => serde_json::from_str(&text).expect("json"),
        other => panic!("expected text, got {other:?}"),
    }
}

#[tokio::test]
async fn a_client_connects_and_is_handed_the_event_carried_for_its_book() {
    let graph = Arc::new(MemoryGraph::new());
    let where_to = serve(graph.clone()).await;

    let (mut client, _) = connect_async(format!("ws://{where_to}/websocket/ws/book:book-1"))
        .await
        .expect("connected");

    // the handshake a client waits for before it trusts the stream
    let hello = next_text(&mut client).await;
    assert_eq!(hello["type"], json!("connected"));
    assert_eq!(hello["channel"], json!("book:book-1"));
    assert!(hello["timestamp"].is_string());

    // the same carry the bus calls when a write happens inside the Book
    let scope = store::BookScope::new("user-1", "book-1");
    let carried = events::realtime::ingest(
        graph.as_ref(),
        &scope,
        hub(),
        &an_event("evt-1", "book-1"),
        events::realtime::DEDUPE_WINDOW,
    )
    .expect("carried");
    assert_eq!(carried["delivered_to"], json!(1));

    let seen = next_text(&mut client).await;
    assert_eq!(seen["type"], json!("event"));
    assert_eq!(seen["event_id"], json!("evt-1"));
    assert_eq!(seen["book_id"], json!("book-1"));
    assert_eq!(seen["resource_type"], json!("journal_entry"));
    assert_eq!(seen["action"], json!("created"));
    // the event stated its own moment, so the client is shown that rather than the moment it was carried
    assert_eq!(seen["timestamp"], json!("2026-11-14T09:00:00Z"));

    // an event for another Book never reaches this client
    let elsewhere = events::realtime::ingest(
        graph.as_ref(),
        &scope,
        hub(),
        &an_event("evt-2", "book-2"),
        events::realtime::DEDUPE_WINDOW,
    )
    .expect("carried");
    assert_eq!(elsewhere["delivered_to"], json!(0));

    // the same event twice is carried once
    let twice = events::realtime::ingest(
        graph.as_ref(),
        &scope,
        hub(),
        &an_event("evt-1", "book-1"),
        events::realtime::DEDUPE_WINDOW,
    )
    .expect("carried");
    assert_eq!(twice["delivered_to"], json!(0));
}

#[tokio::test]
async fn one_client_message_reaches_the_rest_of_the_channel_and_a_departure_is_announced() {
    let graph = Arc::new(MemoryGraph::new());
    let where_to = serve(graph).await;

    let (mut first, _) = connect_async(format!("ws://{where_to}/websocket/ws/book:book-7"))
        .await
        .expect("first connected");
    let (mut second, _) = connect_async(format!("ws://{where_to}/websocket/ws/book:book-7"))
        .await
        .expect("second connected");
    let (mut elsewhere, _) = connect_async(format!("ws://{where_to}/websocket/ws/book:book-8"))
        .await
        .expect("another book connected");

    for socket in [&mut first, &mut second, &mut elsewhere] {
        let hello = next_text(socket).await;
        assert_eq!(hello["type"], json!("connected"));
    }

    // what one client writes is carried to the channel
    first
        .send(Message::Text("typing...".to_string()))
        .await
        .expect("sent");
    let carried = next_text(&mut second).await;
    assert_eq!(carried["channel"], json!("book:book-7"));
    assert_eq!(carried["data"], json!("typing..."));
    assert!(carried["timestamp"].is_string());
    // the writer is on the channel too, so it hears the same copy - which is what the shipped service did and what a
    // collaborative view relies on
    let echo = next_text(&mut first).await;
    assert_eq!(echo["data"], json!("typing..."));

    // a different Book's client sees nothing of it
    let nothing =
        tokio::time::timeout(std::time::Duration::from_millis(200), elsewhere.next()).await;
    assert!(nothing.is_err(), "the other Book heard a message");

    // when a client goes, the rest of the channel is told so a roster does not keep it
    second.close(None).await.expect("closed");
    drop(second);
    let notice = next_text(&mut first).await;
    assert_eq!(notice["type"], json!("disconnect"));
    assert_eq!(notice["channel"], json!("book:book-7"));

    // and the count of who is listening follows: one client left on that channel, the other Book's untouched
    let counted = hub().channels();
    assert_eq!(counted.get("book:book-7"), Some(&1));
    assert_eq!(counted.get("book:book-8"), Some(&1));
}
