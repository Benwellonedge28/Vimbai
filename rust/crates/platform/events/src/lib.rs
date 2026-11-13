#![forbid(unsafe_code)]
//! The Book event bus.
//!
//! * *message-bus-service* ([`events`]): publishing an event inside a Book, the subscriptions that listen for it, and a
//!   record of every delivery attempt.
//!
//! Reaching a subscriber is not this crate's job: the webhook crate's [`Sender`](vimbai_webhooks::webhooks::Sender)
//! carries the request, so the fan-out can be checked without a network and prod still delivers for real.

pub mod events;
