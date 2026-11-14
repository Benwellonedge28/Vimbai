#![forbid(unsafe_code)]
//! The sockets this process is holding open.
//!
//! A socket is a live connection, so it belongs to the process that accepted it and cannot be a record in a Book: a row
//! would outlive the process holding the wire. What lives here is therefore the one registry of "who is listening right
//! now", and every count the service reports is read from it:
//!
//! * a connection is opened on ONE channel and is written to only for that channel, so a Book's events reach that
//!   Book's clients and never another tenant's stream;
//! * a socket whose peer has gone is dropped on the next write rather than counted until a timer notices;
//! * the hub is per process and reached through [`hub`], because a second hub would be a second answer to the same
//!   question - and the moment two answers exist, one of them is wrong.
//!
//! The relay state sits beside it for the same reason: the broker consumer updates it and the statistics read it, so an
//! operator is told what the process is actually doing rather than what the deployment hoped it would do.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicI64, AtomicU64, AtomicU8, Ordering};
use std::sync::{Mutex, OnceLock};

use serde_json::Value;
use tokio::sync::mpsc::UnboundedSender;

/// One open connection: what to write to, and which channel it asked for.
struct Socket {
    id: u64,
    channel: String,
    out: UnboundedSender<Value>,
}

/// Every socket this process holds, keyed by channel.
#[derive(Default)]
pub struct Hub {
    next_id: AtomicU64,
    sockets: Mutex<Vec<Socket>>,
}

impl Hub {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a socket on a channel, answering the id needed to close it again.
    pub fn open(&self, channel: &str, out: UnboundedSender<Value>) -> u64 {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst) + 1;
        self.lock().push(Socket {
            id,
            channel: channel.to_owned(),
            out,
        });
        id
    }

    /// A socket that has gone away is forgotten; its channel keeps whoever is left.
    pub fn close(&self, id: u64) {
        self.lock().retain(|held| held.id != id);
    }

    /// Write one message to every socket on a channel, answering how many were written to.
    ///
    /// A peer that has gone is dropped here rather than raising: a dead client must never fail the write, or the event
    /// that carried it, that the bus already promised to deliver.
    pub fn send(&self, channel: &str, message: &Value) -> usize {
        let mut written = 0usize;
        self.lock().retain(|socket| {
            if socket.channel != channel {
                return true;
            }
            match socket.out.send(message.clone()) {
                Ok(()) => {
                    written += 1;
                    true
                }
                // a peer that has gone is dropped rather than written to again
                Err(_) => false,
            }
        });
        written
    }

    /// How many sockets are listening on each channel.
    pub fn channels(&self) -> BTreeMap<String, i64> {
        let mut counted: BTreeMap<String, i64> = BTreeMap::new();
        for socket in self.lock().iter() {
            *counted.entry(socket.channel.clone()).or_insert(0) += 1;
        }
        counted
    }

    /// Everything this process is holding, counted.
    pub fn total(&self) -> i64 {
        self.lock().len() as i64
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<Socket>> {
        // a panic while the list is held would poison it; the list is still the truth, so it is taken back rather than
        // taking the whole process down over one panic in a handler
        match self.sockets.lock() {
            Ok(held) => held,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

/// What the broker consumer is doing: the three states an operator can act on, plus "error" with the reason.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RelayState {
    /// Nothing asked for a broker, so single-pod delivery over HTTP is all there is.
    Disabled,
    /// A broker was asked for and the consumer is not connected yet.
    Connecting,
    /// Connected, bound and taking events.
    Consuming,
    /// The consumer dropped and is waiting to try again.
    Error,
}

impl RelayState {
    pub fn as_str(self) -> &'static str {
        match self {
            RelayState::Disabled => "disabled",
            RelayState::Connecting => "connecting",
            RelayState::Consuming => "consuming",
            RelayState::Error => "error",
        }
    }
}

/// Where the relay stands, written by the consumer and read by `/stats`.
#[derive(Default)]
pub struct Relay {
    state: AtomicU8,
    received: AtomicI64,
    last_error: Mutex<Option<String>>,
}

impl Relay {
    pub fn set(&self, state: RelayState) {
        self.state.store(
            match state {
                RelayState::Disabled => 0,
                RelayState::Connecting => 1,
                RelayState::Consuming => 2,
                RelayState::Error => 3,
            },
            Ordering::SeqCst,
        );
    }

    pub fn state(&self) -> RelayState {
        match self.state.load(Ordering::SeqCst) {
            1 => RelayState::Connecting,
            2 => RelayState::Consuming,
            3 => RelayState::Error,
            _ => RelayState::Disabled,
        }
    }

    /// An attempt that failed: the reason is kept short, because it is read by a person on a dashboard.
    pub fn failed(&self, reason: &str) {
        self.set(RelayState::Error);
        if let Ok(mut held) = self.last_error.lock() {
            let mut short: String = reason.chars().take(300).collect();
            short.truncate(short.trim_end().len());
            *held = Some(short);
        }
    }

    /// A clean connection clears the last failure, so a stale reason does not outlive the fault it described.
    pub fn recovered(&self) {
        self.set(RelayState::Consuming);
        if let Ok(mut held) = self.last_error.lock() {
            *held = None;
        }
    }

    pub fn received(&self) -> i64 {
        self.received.load(Ordering::SeqCst)
    }

    /// One event taken off the broker.
    pub fn took_one(&self) {
        self.received.fetch_add(1, Ordering::SeqCst);
    }

    pub fn last_error(&self) -> Option<String> {
        self.last_error.lock().ok().and_then(|held| held.clone())
    }
}

static HUB: OnceLock<Hub> = OnceLock::new();
static RELAY: OnceLock<Relay> = OnceLock::new();

/// The process's sockets. One hub, so one answer to "who is listening".
pub fn hub() -> &'static Hub {
    HUB.get_or_init(Hub::new)
}

/// The process's broker consumer state.
pub fn relay() -> &'static Relay {
    RELAY.get_or_init(Relay::default)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn hub_with(channel: &str) -> (Hub, u64, tokio::sync::mpsc::UnboundedReceiver<Value>) {
        let hub = Hub::new();
        let (out, into) = tokio::sync::mpsc::unbounded_channel();
        let id = hub.open(channel, out);
        (hub, id, into)
    }

    #[test]
    fn a_message_reaches_only_the_sockets_of_its_own_channel() {
        let (out_a, mut into_a) = tokio::sync::mpsc::unbounded_channel();
        let (out_b, mut into_b) = tokio::sync::mpsc::unbounded_channel();
        let hub = Hub::new();
        hub.open("book:one", out_a);
        hub.open("book:two", out_b);

        assert_eq!(hub.send("book:one", &json!({"n": 1})), 1);
        assert_eq!(into_a.try_recv().expect("delivered")["n"], json!(1));
        // the other tenant's socket heard nothing at all
        assert!(into_b.try_recv().is_err());
    }

    #[test]
    fn every_socket_on_a_channel_is_written_to_and_counted() {
        let (out_a, mut into_a) = tokio::sync::mpsc::unbounded_channel();
        let (out_b, mut into_b) = tokio::sync::mpsc::unbounded_channel();
        let hub = Hub::new();
        hub.open("book:one", out_a);
        hub.open("book:one", out_b);

        assert_eq!(hub.send("book:one", &json!({"n": 2})), 2);
        assert!(into_a.try_recv().is_ok());
        assert!(into_b.try_recv().is_ok());
        assert_eq!(hub.channels().get("book:one"), Some(&2));
        assert_eq!(hub.total(), 2);
    }

    #[test]
    fn a_closed_socket_stops_being_counted_and_stops_receiving() {
        let (hub, id, mut into) = hub_with("book:one");
        hub.close(id);
        assert_eq!(hub.send("book:one", &json!({"n": 3})), 0);
        assert!(into.try_recv().is_err());
        assert_eq!(hub.total(), 0);
        assert!(hub.channels().is_empty());
    }

    #[test]
    fn a_peer_that_has_gone_is_dropped_on_the_next_write_rather_than_failing_it() {
        let (hub, _id, into) = hub_with("book:one");
        drop(into);
        let (alive, mut into_alive) = tokio::sync::mpsc::unbounded_channel();
        hub.open("book:one", alive);

        // the dead socket neither counts nor raises
        assert_eq!(hub.send("book:one", &json!({"n": 4})), 1);
        assert_eq!(
            into_alive.try_recv().expect("the living socket")["n"],
            json!(4)
        );
        assert_eq!(hub.total(), 1);
    }

    #[test]
    fn the_relay_reports_what_it_is_doing() {
        let relay = Relay::default();
        assert_eq!(relay.state(), RelayState::Disabled);
        relay.set(RelayState::Connecting);
        assert_eq!(relay.state().as_str(), "connecting");
        relay.failed("broker refused the connection");
        assert_eq!(relay.state().as_str(), "error");
        assert_eq!(
            relay.last_error().as_deref(),
            Some("broker refused the connection")
        );
        relay.took_one();
        relay.took_one();
        assert_eq!(relay.received(), 2);
        // a clean connection clears the failure it recovered from
        relay.recovered();
        assert_eq!(relay.state().as_str(), "consuming");
        assert_eq!(relay.last_error(), None);
        // the counter is lifetime, not per connection
        assert_eq!(relay.received(), 2);
    }

    #[test]
    fn one_hub_is_shared_by_the_whole_process() {
        assert!(std::ptr::eq(hub(), hub()));
        assert!(std::ptr::eq(relay(), relay()));
    }
}
