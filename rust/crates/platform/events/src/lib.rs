#![forbid(unsafe_code)]
//! The Book event bus.
//!
//! * *message-bus-service* ([`events`]): publishing an event inside a Book, the subscriptions that listen for it, and a
//!   record of every delivery attempt.
//! * *websocket-service* ([`realtime`]): the last mile of the same stream - the channel a Book's events belong on, the
//!   message a client sees, and one copy even when the same event arrives twice.
//!
//! Reaching a subscriber is not this crate's job: the webhook crate's [`Sender`](vimbai_webhooks::webhooks::Sender)
//! carries the request, so the fan-out can be checked without a network and prod still delivers for real.

pub mod events;
pub mod realtime;
