#![forbid(unsafe_code)]
//! The broker: how one published event reaches every pod, not the one a load balancer happened to pick.
//!
//! A process can carry an event to its own sockets, which is all single-pod real time ever needed. Across replicas the
//! same event has to arrive at each of them, and the only way to do that without every pod knowing every other pod is a
//! fan-out the publisher does not address: a topic exchange where each pod declares its OWN queue. A publish then lands
//! on every queue, so a client is reached wherever its socket happens to be held.
//!
//! Two rules this module refuses to break:
//!
//! * a bus publish NEVER fails because the broker is down. The event is already stored, and the local carry has already
//!   happened; a broker outage costs cross-pod delivery for its duration and nothing else.
//! * the relay says what it is doing. `consuming` only ever means a live consumer with a bound queue, `error` carries the
//!   reason it dropped, and no broker being asked for is reported as `disabled` rather than dressed up as healthy.
//!
//! The connection is re-established with a growing wait, capped, so a broker that comes back is picked up without an
//! operator restarting every pod.

use std::time::Duration;

use futures_util::StreamExt;
use lapin::options::{
    BasicConsumeOptions, BasicPublishOptions, ExchangeDeclareOptions, QueueBindOptions,
    QueueDeclareOptions,
};
use lapin::types::FieldTable;
use lapin::{BasicProperties, Channel, Connection, ConnectionProperties, ExchangeKind};
use serde_json::Value;

use crate::sockets::{relay, RelayState};

/// The exchange the bus publishes to, and every pod's own queue listens on.
pub const EXCHANGE: &str = "vimbai_events";

/// The routing key a pod binds with: everything the bus publishes, whatever the event type.
pub const ROUTING_KEY: &str = "#";

/// Where the broker is, if one was asked for.
///
/// An explicit `RABBITMQ_URL` wins; otherwise the parts the deployment sets are assembled. Nothing at all means no
/// broker was asked for, which is a normal single-pod install rather than a fault, so it is reported as disabled.
pub fn url_from_env() -> Option<String> {
    url_from(|name| std::env::var(name).ok())
}

/// The same rules, read through a lookup instead of the process environment. The environment is process-wide, so two
/// tests setting the same variable run against each other; taking the lookup as an argument lets the rules be tested
/// without touching it, and lets a caller other than the process supply its own configuration.
pub fn url_from<F>(lookup: F) -> Option<String>
where
    F: Fn(&str) -> Option<String>,
{
    let named = |name: &str| lookup(name).filter(|value| !value.trim().is_empty());
    if let Some(explicit) = named("RABBITMQ_URL") {
        return Some(explicit);
    }
    let host = named("RABBITMQ_HOST")?;
    let port = named("RABBITMQ_PORT").unwrap_or_else(|| "5672".to_string());
    let user = named("RABBITMQ_USER").unwrap_or_else(|| "guest".to_string());
    let password = named("RABBITMQ_PASS").unwrap_or_else(|| "guest".to_string());
    Some(format!("amqp://{user}:{password}@{host}:{port}/"))
}

/// How long to wait before the next attempt, doubling from a second and capped so a recovery is noticed quickly.
pub fn backoff(attempt: u32) -> Duration {
    let seconds = 1u64 << attempt.min(5);
    Duration::from_secs(seconds.min(30))
}

/// The queue this pod listens on: its own, exclusive, gone when the pod goes.
fn queue_name() -> String {
    format!("vimbai-realtime-{}", std::process::id())
}

/// Start taking events off the broker.
///
/// `carry` is handed every event the broker delivers, and is responsible for what happens next. It is called on the
/// consumer's own task, so a slow carry delays that pod's fan-out and nothing else.
pub fn start_consuming<F>(url: String, carry: F)
where
    F: Fn(Value) + Send + Sync + 'static,
{
    tokio::spawn(async move {
        let mut attempt: u32 = 0;
        loop {
            relay().set(RelayState::Connecting);
            match consume(&url, &carry).await {
                // a clean return means the consumer ended, which is a drop as far as the stream is concerned
                Ok(()) => relay().failed("the consumer stopped"),
                Err(reason) => relay().failed(&reason),
            }
            let wait = backoff(attempt);
            attempt = attempt.saturating_add(1);
            tokio::time::sleep(wait).await;
        }
    });
}

/// One connection's worth of consuming: connect, declare, bind, and hand over what arrives.
async fn consume<F>(url: &str, carry: &F) -> Result<(), String>
where
    F: Fn(Value) + Sync,
{
    let connection = Connection::connect(url, ConnectionProperties::default())
        .await
        .map_err(|error| format!("cannot reach the broker: {error}"))?;
    let channel = connection
        .create_channel()
        .await
        .map_err(|error| format!("cannot open a broker channel: {error}"))?;

    // durable, because an event published while a pod is restarting should still be there when its queue is declared
    channel
        .exchange_declare(
            EXCHANGE,
            ExchangeKind::Topic,
            ExchangeDeclareOptions {
                durable: true,
                ..ExchangeDeclareOptions::default()
            },
            FieldTable::default(),
        )
        .await
        .map_err(|error| format!("cannot declare {EXCHANGE}: {error}"))?;

    let declared = channel
        .queue_declare(
            queue_name().as_str(),
            QueueDeclareOptions {
                // this pod's own queue: nobody else may take from it, and it goes when the pod does
                exclusive: true,
                auto_delete: true,
                ..QueueDeclareOptions::default()
            },
            FieldTable::default(),
        )
        .await
        .map_err(|error| format!("cannot declare the pod queue: {error}"))?;

    channel
        .queue_bind(
            declared.name().as_str(),
            EXCHANGE,
            ROUTING_KEY,
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await
        .map_err(|error| format!("cannot bind to {EXCHANGE}: {error}"))?;

    let mut deliveries = channel
        .basic_consume(
            declared.name().as_str(),
            "vimbai-realtime",
            BasicConsumeOptions {
                // one at a time: the carry is a fan-out to sockets, and prefetching would only queue work behind it
                no_ack: true,
                ..BasicConsumeOptions::default()
            },
            FieldTable::default(),
        )
        .await
        .map_err(|error| format!("cannot consume: {error}"))?;

    // only now is anything actually being consumed, so only now does the relay say so
    relay().recovered();

    while let Some(delivery) = deliveries.next().await {
        let delivery = match delivery {
            Ok(delivery) => delivery,
            // a dropped connection is the retry loop's business, not a bad event's
            Err(error) => return Err(format!("the broker delivered an error: {error}")),
        };
        relay().took_one();
        // an event this build cannot read is skipped rather than allowed to stop the stream
        if let Ok(event) = serde_json::from_slice::<Value>(&delivery.data) {
            carry(event);
        }
    }
    Ok(())
}

/// The publisher's own connection, kept for the life of the process.
///
/// A connection per publish would open a socket for every ledger write, so the channel is cached and only replaced when
/// it is no longer usable. It is shared across tasks by cloning the handle, which is what the broker client is built for.
static PUBLISHER: std::sync::Mutex<Option<(String, Channel)>> = std::sync::Mutex::new(None);

async fn cached(url: &str) -> Result<Channel, String> {
    if let Ok(held) = PUBLISHER.lock() {
        if let Some((known, channel)) = held.as_ref() {
            if known == url && channel.status().connected() {
                return Ok(channel.clone());
            }
        }
    }
    let connection = Connection::connect(url, ConnectionProperties::default())
        .await
        .map_err(|error| format!("cannot reach the broker: {error}"))?;
    let channel = connection
        .create_channel()
        .await
        .map_err(|error| format!("cannot open a broker channel: {error}"))?;
    declare_exchange(&channel).await?;
    if let Ok(mut held) = PUBLISHER.lock() {
        *held = Some((url.to_string(), channel.clone()));
    }
    Ok(channel)
}

/// Hand one published event to the broker so every other pod's queue gets a copy.
///
/// Called from the publish path, which is synchronous and must never wait on a network: the work is handed to a task and
/// its outcome deliberately not observed, because the event is already stored and already carried locally. No broker
/// configured, or no async runtime to hand it to, simply means there is nothing to do.
pub fn to_broker(event: &Value) {
    let Some(url) = url_from_env() else {
        return;
    };
    let Ok(handle) = tokio::runtime::Handle::try_current() else {
        // a caller with no runtime is a library test, not a live bus: there is no queue to reach
        return;
    };
    let event = event.clone();
    handle.spawn(async move {
        if let Ok(channel) = cached(&url).await {
            let body = event.to_string();
            let sent = channel
                .basic_publish(
                    EXCHANGE,
                    ROUTING_KEY,
                    BasicPublishOptions::default(),
                    body.as_bytes(),
                    BasicProperties::default(),
                )
                .await;
            if let Err(error) = sent {
                // a broker that refused is reported on the relay, where an operator is already looking
                relay().failed(&format!("cannot publish: {error}"));
            }
        }
    });
}

/// Publish one event to the exchange, so every pod's queue gets a copy.
///
/// This is deliberately best-effort: the caller's event is already stored and already carried locally, so a broker that
/// is unreachable costs cross-pod delivery and never a failed write.
pub async fn publish(url: &str, event: &Value) -> Result<(), String> {
    let connection = Connection::connect(url, ConnectionProperties::default())
        .await
        .map_err(|error| format!("cannot reach the broker: {error}"))?;
    let channel = connection
        .create_channel()
        .await
        .map_err(|error| format!("cannot open a broker channel: {error}"))?;
    declare_exchange(&channel).await?;
    let body = event.to_string();
    channel
        .basic_publish(
            EXCHANGE,
            ROUTING_KEY,
            BasicPublishOptions::default(),
            body.as_bytes(),
            BasicProperties::default(),
        )
        .await
        .map_err(|error| format!("cannot publish: {error}"))?
        // the broker's answer, so a message that was never accepted is not reported as delivered
        .await
        .map_err(|error| format!("the broker did not confirm the publish: {error}"))?;
    Ok(())
}

async fn declare_exchange(channel: &Channel) -> Result<(), String> {
    channel
        .exchange_declare(
            EXCHANGE,
            ExchangeKind::Topic,
            ExchangeDeclareOptions {
                durable: true,
                ..ExchangeDeclareOptions::default()
            },
            FieldTable::default(),
        )
        .await
        .map_err(|error| format!("cannot declare {EXCHANGE}: {error}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wait_between_attempts_grows_and_is_capped() {
        assert_eq!(backoff(0), Duration::from_secs(1));
        assert_eq!(backoff(1), Duration::from_secs(2));
        assert_eq!(backoff(3), Duration::from_secs(8));
        // capped, so a pod never waits longer than half a minute to notice the broker is back
        assert_eq!(backoff(10), Duration::from_secs(30));
    }

    /// The variables a test deployment set, read the way the broker reads the environment. No process-wide state, so
    /// these tests cannot collide with each other the way setting real variables did.
    fn deployment(pairs: &[(&str, &str)]) -> Option<String> {
        let pairs: Vec<(String, String)> = pairs
            .iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect();
        url_from(|name| {
            pairs
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.clone())
        })
    }

    #[test]
    fn a_deployment_that_sets_nothing_asked_for_no_broker() {
        assert_eq!(deployment(&[]), None);
        // a host that was set to nothing is not a host
        assert_eq!(deployment(&[("RABBITMQ_HOST", "  ")]), None);
    }

    #[test]
    fn an_explicit_url_wins_and_the_parts_are_assembled_when_it_is_absent() {
        assert_eq!(
            deployment(&[
                ("RABBITMQ_URL", "amqp://user:secret@bus:5672/vh"),
                ("RABBITMQ_HOST", "ignored"),
            ])
            .as_deref(),
            Some("amqp://user:secret@bus:5672/vh")
        );
        assert_eq!(
            deployment(&[
                ("RABBITMQ_HOST", "bus"),
                ("RABBITMQ_USER", "vimbai"),
                ("RABBITMQ_PASS", "secret"),
            ])
            .as_deref(),
            Some("amqp://vimbai:secret@bus:5672/")
        );
        // the shipped defaults, so a compose file need only name the host
        assert_eq!(
            deployment(&[("RABBITMQ_HOST", "bus")]).as_deref(),
            Some("amqp://guest:guest@bus:5672/")
        );
        assert_eq!(
            deployment(&[("RABBITMQ_HOST", "bus"), ("RABBITMQ_PORT", "5673")]).as_deref(),
            Some("amqp://guest:guest@bus:5673/")
        );
    }

    #[test]
    fn each_pod_listens_on_its_own_queue() {
        // exclusive and named after the process, so two pods never take each other's events
        assert!(queue_name().starts_with("vimbai-realtime-"));
        assert!(queue_name().contains(&std::process::id().to_string()));
    }
}
