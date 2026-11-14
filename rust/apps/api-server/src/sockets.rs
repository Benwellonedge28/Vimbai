#![forbid(unsafe_code)]
//! The socket a client actually holds: `GET /ws/{channel}`.
//!
//! Everything about the stream is decided in the events crate - which channel a Book's events belong on, the shape of the
//! message, and whether a copy is a duplicate. What is left is the wire: accept the upgrade, say hello with the same
//! handshake the shipped service sent, carry what the client writes to the rest of the channel, and tell the channel when
//! it goes. Three behaviours are kept because a connected client already depends on them:
//!
//! * the first frame is `{"type":"connected","channel":...,"timestamp":...}`, so a client knows it is attached rather
//!   than inferring it from silence;
//! * whatever the client writes is carried to everyone on the channel as `{"channel":...,"data":<text>,"timestamp":...}`,
//!   which is how a collaboration view stays in step;
//! * a socket that goes away is announced as `{"type":"disconnect",...}`, so the other clients can drop it from a roster.

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::Path;
use axum::response::Response;
use serde_json::{json, Value};

use common::{clock, dates};
use events::sockets::{hub, Hub};

/// Upgrade the connection and hold it open on its channel.
pub async fn socket(upgrade: WebSocketUpgrade, Path(channel): Path<String>) -> Response {
    upgrade.on_upgrade(move |socket| hold(socket, channel))
}

async fn hold(mut socket: WebSocket, channel: String) {
    if channel.trim().is_empty() {
        // no channel, no stream: the connection is closed rather than silently attached to nothing
        let _ = socket.send(Message::Close(None)).await;
        return;
    }

    let sockets = hub();
    let (out, mut into) = tokio::sync::mpsc::unbounded_channel::<Value>();
    let id = sockets.open(&channel, out);

    let attached = json!({
        "type": "connected",
        "channel": channel,
        "timestamp": dates::written_at(clock::moment()),
    });
    if socket
        .send(Message::Text(attached.to_string()))
        .await
        .is_err()
    {
        sockets.close(id);
        return;
    }

    loop {
        tokio::select! {
            // something to write to this client: an event carried for its Book, or another client's message
            Some(message) = into.recv() => {
                if socket.send(Message::Text(message.to_string())).await.is_err() {
                    break;
                }
            }
            // something this client wrote, to be carried to the rest of the channel
            incoming = socket.recv() => match incoming {
                Some(Ok(Message::Text(text))) => relay(sockets, &channel, &text),
                // a client that speaks bytes is carried as text, because text is what the channel speaks
                Some(Ok(Message::Binary(bytes))) => {
                    if let Ok(text) = String::from_utf8(bytes) {
                        relay(sockets, &channel, &text);
                    }
                }
                Some(Ok(Message::Ping(_))) | Some(Ok(Message::Pong(_))) => {}
                // a close frame, a protocol error, or the peer vanishing all mean the same thing here
                _ => break,
            },
        }
    }

    sockets.close(id);
    // the rest of the channel is told, so a roster does not keep a client that has gone
    sockets.send(
        &channel,
        &json!({
            "type": "disconnect",
            "channel": channel,
            "timestamp": dates::written_at(clock::moment()),
        }),
    );
}

/// Carry what one client wrote to the rest of its channel.
fn relay(sockets: &Hub, channel: &str, text: &str) {
    sockets.send(
        channel,
        &json!({
            "channel": channel,
            "data": text,
            "timestamp": dates::written_at(clock::moment()),
        }),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_client_message_is_carried_in_the_shape_the_shipped_service_sent() {
        let sockets = Hub::new();
        let (out, mut into) = tokio::sync::mpsc::unbounded_channel();
        sockets.open("book:book-1", out);

        relay(&sockets, "book:book-1", "typing");
        let carried = into.try_recv().expect("carried");
        assert_eq!(carried["channel"], json!("book:book-1"));
        assert_eq!(carried["data"], json!("typing"));
        assert!(carried["timestamp"].is_string());
        assert!(carried["type"].is_null());
    }

    #[test]
    fn a_message_is_carried_to_every_socket_on_the_channel_and_no_other() {
        let sockets = Hub::new();
        let (first, mut one) = tokio::sync::mpsc::unbounded_channel();
        let (second, mut two) = tokio::sync::mpsc::unbounded_channel();
        let (elsewhere, mut other) = tokio::sync::mpsc::unbounded_channel();
        sockets.open("book:book-1", first);
        sockets.open("book:book-1", second);
        sockets.open("book:book-2", elsewhere);

        relay(&sockets, "book:book-1", "hello");
        assert!(one.try_recv().is_ok());
        assert!(two.try_recv().is_ok());
        assert!(other.try_recv().is_err());
    }
}
