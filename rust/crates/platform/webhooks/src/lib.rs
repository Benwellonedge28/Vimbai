#![forbid(unsafe_code)]
//! Webhook service.
//!
//! * *webhook-service* ([`webhooks`]): the endpoints a Book subscribes, the events dispatched to them, and how each
//!   delivery went.
//!
//! Reaching the outside is not this crate's job: a [`Sender`] carries the request, so the service can be checked
//! without a network and prod still delivers for real.

pub mod webhooks;
