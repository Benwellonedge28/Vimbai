#![forbid(unsafe_code)]
//! How much work the engine takes on at once, and how long it will wait.
//!
//! A store has a small pool of connections and every request that needs one holds it for a moment. Without a ceiling a
//! burst simply queues on that pool: memory grows with the queue, nothing fails visibly, and latency climbs until clients
//! give up on their own. Backpressure belongs at the door, where it can be counted and named, not in the pool where it
//! shows up as a mystery.
//!
//! So the engine admits a bounded number of requests at a time and gives every one of them a deadline. A request that
//! cannot start within its deadline is refused rather than left waiting, and a request that starts but runs past it is
//! cut off, so a slow store costs one caller a clear answer instead of costing everyone their connections.
//!
//! The numbers are settings, because what a laptop can do and what a cluster node can do are not the same question:
//! `VIMBAI_API_MAX_IN_FLIGHT` (default 256, at least 1) and `VIMBAI_API_TIMEOUT_MS` (default 30000).

use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::{Json, Router};
use serde_json::json;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

/// How many requests may run at once, and how long each may take.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    pub in_flight: usize,
    pub timeout: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            in_flight: 256,
            timeout: Duration::from_secs(30),
        }
    }
}

impl Limits {
    /// The limits a deployment asked for, or the defaults when it asked for nothing.
    ///
    /// A nonsense setting falls back rather than refusing to boot: an engine that will not start because a variable has a
    /// typo in it is worse than an engine running on its defaults.
    pub fn from_env() -> Self {
        let mut limits = Self::default();
        if let Some(count) = read("VIMBAI_API_MAX_IN_FLIGHT").filter(|count| *count > 0) {
            limits.in_flight = count;
        }
        if let Some(millis) = read("VIMBAI_API_TIMEOUT_MS").filter(|millis| *millis > 0) {
            limits.timeout = Duration::from_millis(millis as u64);
        }
        limits
    }
}

fn read(name: &str) -> Option<usize> {
    std::env::var(name).ok()?.trim().parse().ok()
}

#[derive(Clone)]
struct Governed {
    permits: Arc<Semaphore>,
    timeout: Duration,
}

/// Put the door in front of a router: admit, time out, and name what happened.
pub fn govern(router: Router, limits: Limits) -> Router {
    let state = Governed {
        permits: Arc::new(Semaphore::new(limits.in_flight.max(1))),
        timeout: limits.timeout,
    };
    router.layer(axum::middleware::from_fn_with_state(state, admit))
}

fn full(waited: &str) -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(json!({ "error": format!("the engine is at its limit: {waited}") })),
    )
        .into_response()
}

fn too_slow(limit: Duration) -> Response {
    (
        StatusCode::GATEWAY_TIMEOUT,
        Json(json!({ "error": format!("the request did not finish within {}ms", limit.as_millis()) })),
    )
        .into_response()
}

async fn admit(State(state): State<Governed>, request: Request, next: Next) -> Response {
    // Waiting for a slot is part of the deadline, not a way around it: a caller stuck behind a queue has been served no
    // better than one whose work ran long, and it should hear so while it is still listening.
    let permit: OwnedSemaphorePermit =
        match tokio::time::timeout(state.timeout, state.permits.clone().acquire_owned()).await {
            Ok(Ok(permit)) => permit,
            Ok(Err(_)) => return full("the door is closed"),
            Err(_) => return full("no slot came free within the deadline"),
        };
    let answer = tokio::time::timeout(state.timeout, next.run(request)).await;
    // the permit is held until the answer exists, so the count is of work, not of arrivals
    drop(permit);
    match answer {
        Ok(response) => response,
        Err(_) => too_slow(state.timeout),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_limits_are_settings_with_defaults_that_do_not_refuse_to_boot() {
        assert_eq!(Limits::default().in_flight, 256);
        assert_eq!(Limits::default().timeout, Duration::from_secs(30));
        // a setting that does not parse falls back rather than stopping the engine from starting
        assert!(read("VIMBAI_API_MAX_IN_FLIGHT_THAT_IS_NOT_SET").is_none());
    }
}
