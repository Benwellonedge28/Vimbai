#![forbid(unsafe_code)]
//! message-bus-service: publishing an event inside a Book, the subscriptions that listen for it, and a record of every
//! delivery attempt.
//!
//! The bus is a fan-out, and the fan-out is the part worth pinning:
//!
//! * an event's type is one of a fixed list. Anything else is refused by name rather than stored and never delivered,
//!   because a type nobody listens for is a message that vanishes;
//! * a published event is STORED first and delivered second, so a subscriber that was down can still be caught up from
//!   the store, and a delivery that fails never fails the publish - the event stays on the bus and the failure is
//!   recorded against it;
//! * only the caller's own subscriptions are considered, and only the ENABLED ones that named this event type. A
//!   disabled subscription is not a slower subscription; it is silent;
//! * each attempt is its own immutable record: which subscription, which URL, the answer it gave (a status code) or why
//!   it could not be reached. A delivery record is never rewritten, so "what did we try, and what happened" stays
//!   answerable;
//! * a subscription carrying a secret gets the body signed, HMAC-SHA256 over the exact text sent, the same computation
//!   the webhook service uses - one implementation, shared;
//! * the four trigger endpoints exist for testing a pipeline end to end. They are the same publish with the type and
//!   source filled in, and two of them raise the priority on the shipped thresholds: a budget variance past twenty
//!   percent is high, and a fraud score past eight tenths is critical.
//!
//! The published copy of an event is the record; the broker is a transport. This port therefore stores and fans out, and
//! answers `broker_connected: false` rather than pretending a message queue is behind it.

use serde_json::{json, Map, Value};

use vimbai_common::ids::service_id;
use vimbai_store::read::{flag, number, refused, text, whole};
use vimbai_store::{BookScope, Graph, RecordKind, Records, StoreError};
use vimbai_webhooks::webhooks::{signature, Sender};

/// An event that was published in a Book.
pub const EVENT: RecordKind = RecordKind::new("BusEvent", "OWNS_BUS_EVENT");
/// Someone listening for certain event types.
pub const SUBSCRIPTION: RecordKind = RecordKind::new("BusSubscription", "OWNS_BUS_SUBSCRIPTION");
/// One attempt at telling a subscriber about an event. Write once.
pub const DELIVERY: RecordKind = RecordKind::new("BusDelivery", "OWNS_BUS_DELIVERY");

/// Every event type the bus knows. A type outside this list is refused rather than published to nobody.
pub const EVENT_TYPES: [&str; 36] = [
    "accounting.journal_entry.created",
    "accounting.journal_entry.updated",
    "accounting.journal_entry.posted",
    "accounting.journal_entry.deleted",
    "accounting.account.created",
    "accounting.account.updated",
    "accounting.trial_balance.generated",
    "finance.budget.created",
    "finance.budget.updated",
    "finance.budget.variance_alert",
    "finance.scenario.created",
    "banking.transaction.created",
    "banking.transaction.reconciled",
    "banking.transaction.updated",
    "banking.transaction.deleted",
    "fraud.transaction.flagged",
    "workflow.approval.requested",
    "workflow.approval.completed",
    "workflow.approval.rejected",
    "integration.pos.sync_completed",
    "integration.bank_feed.received",
    "integration.inventory.updated",
    "multimodal.document.processed",
    "multimodal.voice.transcript_complete",
    "book.resource.created",
    "book.resource.updated",
    "book.resource.deleted",
    "system.service.healthy",
    "system.service.unhealthy",
    "system.feature.toggled",
    "system.data_sync.completed",
    "accounting.journal_entry.reversed",
    "banking.statement.imported",
    "treasury.payment.scheduled",
    "npo.donation.received",
    "audit.event.recorded",
];

/// The priorities an event can carry.
pub const PRIORITIES: [&str; 4] = ["low", "normal", "high", "critical"];

fn events_of(payload: &Value) -> Vec<String> {
    match payload.get("event_types") {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect(),
        Some(Value::String(one)) => one
            .split(',')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .map(str::to_string)
            .collect(),
        _ => Vec::new(),
    }
}

fn checked_type(event_type: &str) -> Result<String, StoreError> {
    if !EVENT_TYPES.contains(&event_type) {
        // a type nobody could ever listen for is refused, not stored
        return Err(refused(&format!("unknown event type: {event_type}")));
    }
    Ok(event_type.to_string())
}

fn priority_of(payload: &Value) -> String {
    let priority = text(payload, "priority");
    if PRIORITIES.contains(&priority.as_str()) {
        priority
    } else {
        "normal".to_string()
    }
}

/// The body a subscriber receives: the event as it was stored, without the retry bookkeeping the bus keeps for itself.
fn delivered_body(event: &Value) -> String {
    let mut body = event.clone();
    if let Some(object) = body.as_object_mut() {
        object.remove("retry_count");
        object.remove("max_retries");
    }
    body.to_string()
}

/// Record one attempt, and it is never revised afterwards.
fn record_delivery(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    event_id: &str,
    subscription_id: &str,
    callback_url: &str,
    outcome: &Result<u16, String>,
) -> Result<Value, StoreError> {
    let mut fields = json!({
        "id": service_id(),
        "event_id": event_id,
        "subscription_id": subscription_id,
        "callback_url": callback_url,
    });
    match outcome {
        Ok(code) => {
            fields["status"] = json!(if *code < 400 { "delivered" } else { "failed" });
            fields["response_status"] = json!(code);
        }
        Err(reason) => {
            fields["status"] = json!("failed");
            fields["error"] = json!(reason.chars().take(500).collect::<String>());
        }
    }
    Records::new(graph, DELIVERY).create(scope, &fields)
}

/// Fan one event out to the caller's subscriptions, and answer what happened.
fn dispatch(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    event: &Value,
    sender: &dyn Sender,
    now: &Value,
) -> Result<Value, StoreError> {
    let event_type = text(event, "type");
    let body = delivered_body(event);
    let event_id = text(event, "id");
    let mut delivered = 0;
    let mut failed = 0;
    for subscription in Records::new(graph, SUBSCRIPTION).list(scope, 500)? {
        if !flag(&subscription, "enabled") {
            // switched off means silent, not slow
            continue;
        }
        if !events_of(&subscription).contains(&event_type) {
            continue;
        }
        let secret = text(&subscription, "secret");
        let mut headers = vec![
            ("Content-Type".to_string(), "application/json".to_string()),
            ("X-Vimbai-Event".to_string(), event_type.clone()),
        ];
        if !secret.is_empty() {
            headers.push(("X-Vimbai-Signature".to_string(), signature(&secret, &body)));
        }
        let url = text(&subscription, "callback_url");
        let outcome = sender.post(&url, &body, &headers);
        let made = record_delivery(
            graph,
            scope,
            &event_id,
            &text(&subscription, "id"),
            &url,
            &outcome,
        )?;
        if text(&made, "status") == "delivered" {
            delivered += 1;
        } else {
            failed += 1;
        }
    }
    Ok(json!({
        "delivered": delivered,
        "failed": failed,
        "timestamp": now,
    }))
}

/// Store the event, then fan it out. A delivery that fails never fails the publish.
fn publish_one(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    event: Value,
    sender: &dyn Sender,
    now: &Value,
) -> Result<Value, StoreError> {
    let event = Records::new(graph, EVENT).create(scope, &event)?;
    // the other pods hear about it too, which is what makes a client reachable wherever its socket is held. Handed off
    // rather than waited on: the event is stored, and a broker that is down must never fail a publish.
    crate::broker::to_broker(&carried_event(&event, scope));
    let dispatch = dispatch(graph, scope, &event, sender, now)?;
    Ok(json!({
        "status": "published",
        "event_id": text(&event, "id"),
        "event_type": text(&event, "type"),
        "dispatch": dispatch,
    }))
}

/// The event as the other pods should see it: the stored event with the Book it belongs to, because a pod receiving it
/// has no request to read a scope from.
fn carried_event(event: &Value, scope: &BookScope) -> Value {
    let mut carried = event.clone();
    if let Some(payload) = carried.get_mut("payload").and_then(Value::as_object_mut) {
        payload
            .entry("book_id")
            .or_insert_with(|| json!(scope.book_id));
        payload
            .entry("user_id")
            .or_insert_with(|| json!(scope.user_id));
    }
    carried
}

/// `POST /bus/events/publish`: publish a fully described event.
pub fn publish(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
    sender: &dyn Sender,
) -> Result<Value, StoreError> {
    let event_type = checked_type(&text(payload, "type"))?;
    let supplied = text(payload, "id");
    let event = json!({
        "id": if supplied.is_empty() { service_id() } else { supplied },
        "type": event_type,
        "source_service": text(payload, "source_service"),
        "timestamp": payload.get("timestamp").cloned().unwrap_or(Value::Null),
        "priority": priority_of(payload),
        "payload": payload.get("payload").cloned().unwrap_or_else(|| json!({})),
        "correlation_id": payload.get("correlation_id").cloned().unwrap_or(Value::Null),
        "reply_to": payload.get("reply_to").cloned().unwrap_or(Value::Null),
        "headers": payload.get("headers").cloned().unwrap_or(Value::Null),
        "retry_count": 0,
        "max_retries": whole(payload, "max_retries"),
    });
    publish_one(
        graph,
        scope,
        event,
        sender,
        &payload.get("now").cloned().unwrap_or(Value::Null),
    )
}

/// An event described by type and body instead of in full - what the convenience door and the triggers carry.
struct Envelope<'a> {
    event_type: &'a str,
    source_service: &'a str,
    body: Value,
    priority: &'a str,
}

/// Publish an event described by type and body alone.
fn publish_of_type(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    envelope: Envelope<'_>,
    payload: &Value,
    sender: &dyn Sender,
) -> Result<Value, StoreError> {
    let Envelope {
        event_type,
        source_service,
        body,
        priority,
    } = envelope;
    let event = json!({
        "id": service_id(),
        "type": checked_type(event_type)?,
        "source_service": source_service,
        "timestamp": payload.get("now").cloned().unwrap_or(Value::Null),
        "priority": priority,
        "payload": body,
        "correlation_id": Value::Null,
        "reply_to": Value::Null,
        "headers": Value::Null,
        "retry_count": 0,
        "max_retries": 3,
    });
    publish_one(
        graph,
        scope,
        event,
        sender,
        &payload.get("now").cloned().unwrap_or(Value::Null),
    )
}

/// `POST /bus/events/{type}/publish`: publish by naming the type in the path.
pub fn publish_by_type(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    event_type: &str,
    payload: &Value,
    sender: &dyn Sender,
) -> Result<Value, StoreError> {
    let priority = priority_of(payload);
    publish_of_type(
        graph,
        scope,
        Envelope {
            event_type,
            source_service: &text(payload, "source_service"),
            body: payload.get("payload").cloned().unwrap_or_else(|| json!({})),
            priority: &priority,
        },
        payload,
        sender,
    )
}

/// `POST /bus/event-subscriptions`: someone starts listening.
pub fn bus_subscribe(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let name = text(payload, "name");
    if name.trim().is_empty() {
        return Err(refused("name is required"));
    }
    let callback_url = text(payload, "callback_url");
    if callback_url.trim().is_empty() {
        return Err(refused("callback_url is required"));
    }
    let mut event_types = Vec::new();
    for named in events_of(payload) {
        event_types.push(json!(checked_type(&named)?));
    }
    if event_types.is_empty() {
        // a subscription that listens for nothing is a mistake worth naming
        return Err(refused("event_types must name at least one event type"));
    }
    let enabled = match payload.get("enabled") {
        Some(Value::Bool(given)) => *given,
        _ => true,
    };
    let priority = priority_of(payload);
    Records::new(graph, SUBSCRIPTION).create(
        scope,
        &json!({
            "id": if text(payload, "id").is_empty() { service_id() } else { text(payload, "id") },
            "name": name,
            "event_types": event_types,
            "callback_url": callback_url,
            "filter_expression": payload.get("filter_expression").cloned().unwrap_or(Value::Null),
            "enabled": enabled,
            "priority": priority,
            "secret": text(payload, "secret"),
        }),
    )
}

/// `GET /bus/event-subscriptions`: everything the caller has listening.
pub fn bus_subscriptions(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
) -> Result<Value, StoreError> {
    Ok(Value::Array(
        Records::new(graph, SUBSCRIPTION).list(scope, 500)?,
    ))
}

/// `GET /bus/event-subscriptions/{id}`: one subscription.
pub fn get_subscription(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    subscription_id: &str,
) -> Result<Value, StoreError> {
    Records::new(graph, SUBSCRIPTION).get(scope, subscription_id)
}

/// `DELETE /bus/event-subscriptions/{id}`: stop listening. Missing and another Book's are both missing.
pub fn delete_subscription(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    subscription_id: &str,
) -> Result<Value, StoreError> {
    Records::new(graph, SUBSCRIPTION).get(scope, subscription_id)?;
    Records::new(graph, SUBSCRIPTION).delete(scope, subscription_id)?;
    Ok(json!({"status": "deleted"}))
}

/// `GET /bus/events`: the Book's events, newest first, filtered as asked.
pub fn bus_events(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let event_type = text(payload, "event_type");
    let source_service = text(payload, "source_service");
    let limit = match payload.get("limit") {
        None | Some(Value::Null) => 100,
        _ => (number(payload, "limit") as i64).clamp(1, 1000),
    };
    let mut rows: Vec<(i64, Value)> = Records::new(graph, EVENT)
        .list(scope, 1000)?
        .into_iter()
        .filter(|event| event_type.is_empty() || text(event, "type") == event_type)
        .filter(|event| {
            source_service.is_empty() || text(event, "source_service") == source_service
        })
        .map(|event| ((number(&event, "timestamp") * 1000.0) as i64, event.clone()))
        .collect();
    rows.sort_by_key(|(order, _)| std::cmp::Reverse(*order));
    rows.truncate(limit as usize);
    Ok(Value::Array(
        rows.into_iter().map(|(_, event)| event).collect(),
    ))
}

/// `GET /bus/events/{id}`: one event.
pub fn bus_event(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    event_id: &str,
) -> Result<Value, StoreError> {
    Records::new(graph, EVENT).get(scope, event_id)
}

/// `GET /bus/events/{id}/deliveries`: every attempt made for one event.
pub fn event_deliveries(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    event_id: &str,
) -> Result<Value, StoreError> {
    Records::new(graph, EVENT).get(scope, event_id)?;
    let mut rows: Vec<(i64, Value)> = Records::new(graph, DELIVERY)
        .list(scope, 1000)?
        .into_iter()
        .filter(|delivery| text(delivery, "event_id") == event_id)
        .map(|delivery| ((number(&delivery, "created_at") * 1000.0) as i64, delivery))
        .collect();
    rows.sort_by_key(|(order, _)| *order);
    Ok(Value::Array(rows.into_iter().map(|(_, row)| row).collect()))
}

/// `GET /event-types`: what the bus can carry.
pub fn event_types() -> Value {
    Value::Array(
        EVENT_TYPES
            .iter()
            .map(|name| json!({"name": name.replace('.', "_").to_uppercase(), "value": name}))
            .collect(),
    )
}

/// `GET /bus/metrics`: what this Book's bus is doing.
pub fn metrics(graph: &(impl Graph + ?Sized), scope: &BookScope) -> Result<Value, StoreError> {
    let events = Records::new(graph, EVENT).list(scope, 1000)?;
    let mut counts = Map::new();
    for event in &events {
        let kind = text(event, "type");
        let held = counts.get(&kind).and_then(Value::as_u64).unwrap_or(0);
        counts.insert(kind, json!(held + 1));
    }
    Ok(json!({
        "total_events": events.len(),
        "event_counts_by_type": Value::Object(counts),
        "total_subscriptions": Records::new(graph, SUBSCRIPTION).list(scope, 500)?.len(),
        // the store is the bus here: no message broker is behind this port
        "broker_connected": false,
    }))
}

/// `POST /bus/trigger/journal-entry-created`: a test hook for the accounting pipeline.
pub fn trigger_journal_entry(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
    sender: &dyn Sender,
) -> Result<Value, StoreError> {
    let made = publish_of_type(
        graph,
        scope,
        Envelope {
            event_type: "accounting.journal_entry.created",
            source_service: "accounting-service",
            body: json!({
            "entry_id": text(payload, "entry_id"),
            "amount": payload.get("amount").cloned().unwrap_or(Value::Null),
                "description": if text(payload, "description").is_empty() {
                    json!("Test journal entry")
                } else {
                    json!(text(payload, "description"))
                },
            }),
            priority: "normal",
        },
        payload,
        sender,
    )?;
    Ok(json!({"status": "triggered", "event_id": made["event_id"]}))
}

/// `POST /bus/trigger/budget-variance-alert`: high once the variance passes twenty percent, as shipped.
pub fn trigger_budget_variance(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
    sender: &dyn Sender,
) -> Result<Value, StoreError> {
    let variance = number(payload, "variance_percent");
    let made = publish_of_type(
        graph,
        scope,
        Envelope {
            event_type: "finance.budget.variance_alert",
            source_service: "finance-service",
            body: json!({
                "budget_id": text(payload, "budget_id"),
                "variance_percent": variance,
                "category": text(payload, "category"),
            }),
            priority: if variance > 20.0 { "high" } else { "normal" },
        },
        payload,
        sender,
    )?;
    Ok(json!({"status": "triggered", "event_id": made["event_id"]}))
}

/// `POST /bus/trigger/transaction-flagged`: critical past eight tenths of a score, high below it.
pub fn trigger_transaction_flagged(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
    sender: &dyn Sender,
) -> Result<Value, StoreError> {
    let score = number(payload, "fraud_score");
    let made = publish_of_type(
        graph,
        scope,
        Envelope {
            event_type: "fraud.transaction.flagged",
            source_service: "fraud-detection-service",
            body: json!({
                "transaction_id": text(payload, "transaction_id"),
                "fraud_score": score,
                "reason": text(payload, "reason"),
            }),
            priority: if score > 0.8 { "critical" } else { "high" },
        },
        payload,
        sender,
    )?;
    Ok(json!({"status": "triggered", "event_id": made["event_id"]}))
}

/// `POST /bus/trigger/approval-requested`: always high, as shipped.
pub fn trigger_approval_requested(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
    sender: &dyn Sender,
) -> Result<Value, StoreError> {
    let made = publish_of_type(
        graph,
        scope,
        Envelope {
            event_type: "workflow.approval.requested",
            source_service: "workflow-service",
            body: json!({
                "approval_id": text(payload, "approval_id"),
                "requester": text(payload, "requester"),
                "approvers": payload.get("approvers").cloned().unwrap_or_else(|| json!([])),
                "amount": payload.get("amount").cloned().unwrap_or(Value::Null),
            }),
            priority: "high",
        },
        payload,
        sender,
    )?;
    Ok(json!({"status": "triggered", "event_id": made["event_id"]}))
}

/// The bus's own health line, which says whether a broker is behind it.
pub fn health(graph: &(impl Graph + ?Sized), scope: &BookScope) -> Result<Value, StoreError> {
    Ok(json!({
        "status": "healthy",
        "service": "message-bus",
        "connected": false,
        "events_stored": Records::new(graph, EVENT).list(scope, 1000)?.len(),
        "subscriptions": Records::new(graph, SUBSCRIPTION).list(scope, 500)?.len(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use vimbai_store::fake::MemoryGraph;

    /// One request a sender was asked to carry: where it was going, the body, and the headers it carried.
    type Posted = (String, String, Vec<(String, String)>);

    /// A sender that records what it was asked to post and answers with a fixed code.
    struct Told {
        answer: Result<u16, String>,
        sent: RefCell<Vec<Posted>>,
    }

    impl Sender for Told {
        fn post(&self, url: &str, body: &str, headers: &[(String, String)]) -> Result<u16, String> {
            self.sent
                .borrow_mut()
                .push((url.to_string(), body.to_string(), headers.to_vec()));
            self.answer.clone()
        }
    }

    fn listener(answer: Result<u16, String>) -> Told {
        Told {
            answer,
            sent: RefCell::new(Vec::new()),
        }
    }

    fn talked() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn listening(graph: &MemoryGraph, secret: &str) {
        bus_subscribe(
            graph,
            &talked(),
            &json!({
                "name": "ledger watcher",
                "event_types": ["accounting.journal_entry.created"],
                "callback_url": "https://example.test/hook",
                "secret": secret
            }),
        )
        .expect("a subscription");
    }

    #[test]
    fn publishing_stores_the_event_and_tells_the_subscriber() {
        let graph = MemoryGraph::new();
        listening(&graph, "shh");
        let sender = listener(Ok(200));
        let published = publish(
            &graph,
            &talked(),
            &json!({"type": "accounting.journal_entry.created", "source_service": "accounting-service",
                    "payload": {"entry_id": "je-1"}, "now": "2026-01-01T00:00:00Z"}),
            &sender,
        )
        .expect("a publish");
        assert_eq!(published["status"], json!("published"));
        assert_eq!(published["dispatch"]["delivered"], json!(1));
        assert_eq!(published["dispatch"]["failed"], json!(0));
        // what was sent is the event, signed, with the type in a header
        let sent = sender.sent.borrow();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].0, "https://example.test/hook");
        assert!(sent[0].1.contains("je-1"));
        assert!(sent[0]
            .2
            .iter()
            .any(|(name, value)| name == "X-Vimbai-Event"
                && value == "accounting.journal_entry.created"));
        assert!(sent[0]
            .2
            .iter()
            .any(|(name, value)| name == "X-Vimbai-Signature"
                && value == &signature("shh", &sent[0].1)));
        // and the event is on the bus afterwards
        let stored = bus_events(&graph, &talked(), &json!({})).expect("events");
        assert_eq!(stored.as_array().map(Vec::len), Some(1));
        assert_eq!(stored[0]["retry_count"], json!(0));
    }

    #[test]
    fn a_failed_delivery_is_recorded_and_the_publish_still_stands() {
        let graph = MemoryGraph::new();
        listening(&graph, "");
        let sender = listener(Err("connection refused".to_string()));
        let published = publish(
            &graph,
            &talked(),
            &json!({"type": "accounting.journal_entry.created", "source_service": "a", "payload": {}}),
            &sender,
        )
        .expect("a publish");
        assert_eq!(published["dispatch"]["failed"], json!(1));
        let event_id = text(&published, "event_id");
        let attempts = event_deliveries(&graph, &talked(), &event_id).expect("deliveries");
        let rows = attempts.as_array().expect("deliveries");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["status"], json!("failed"));
        assert_eq!(rows[0]["error"], json!("connection refused"));
        // an unanswered subscriber is not a lost event
        assert!(bus_event(&graph, &talked(), &event_id).is_ok());
    }

    #[test]
    fn only_enabled_subscribers_of_that_type_are_told() {
        let graph = MemoryGraph::new();
        listening(&graph, "");
        bus_subscribe(
            &graph,
            &talked(),
            &json!({"name": "budgets only", "event_types": ["finance.budget.created"],
                    "callback_url": "https://example.test/other"}),
        )
        .expect("a subscription");
        bus_subscribe(
            &graph,
            &talked(),
            &json!({"name": "switched off", "event_types": ["accounting.journal_entry.created"],
                    "callback_url": "https://example.test/off", "enabled": false}),
        )
        .expect("a subscription");
        let sender = listener(Ok(204));
        publish(
            &graph,
            &talked(),
            &json!({"type": "accounting.journal_entry.created", "source_service": "a", "payload": {}}),
            &sender,
        )
        .expect("a publish");
        // one listener heard it; the one on another type and the switched-off one did not
        assert_eq!(sender.sent.borrow().len(), 1);
        // and a type outside the list is refused by name
        let refused_publish = publish(
            &graph,
            &talked(),
            &json!({"type": "accounting.made.up", "source_service": "a", "payload": {}}),
            &sender,
        );
        assert!(refused_publish.is_err());
        assert_eq!(sender.sent.borrow().len(), 1);
    }

    #[test]
    fn a_subscription_must_name_a_type_the_bus_can_carry() {
        let graph = MemoryGraph::new();
        assert!(bus_subscribe(
            &graph,
            &talked(),
            &json!({"name": "nobody", "event_types": [], "callback_url": "https://example.test/x"})
        )
        .is_err());
        assert!(bus_subscribe(
            &graph,
            &talked(),
            &json!({"name": "odd", "event_types": ["made.up"], "callback_url": "https://example.test/x"})
        )
        .is_err());
        assert!(bus_subscribe(
            &graph,
            &talked(),
            &json!({"name": "nowhere", "event_types": ["finance.budget.created"], "callback_url": ""})
        )
        .is_err());
        // another Book's subscription is missing, not readable
        bus_subscribe(
            &graph,
            &talked(),
            &json!({"name": "mine", "event_types": ["finance.budget.created"],
                    "callback_url": "https://example.test/x"}),
        )
        .expect("a subscription");
        let listed = bus_subscriptions(&graph, &talked()).expect("subscriptions");
        let id = text(&listed[0], "id");
        let elsewhere = BookScope::new("user-2", "book-2");
        assert!(get_subscription(&graph, &elsewhere, &id).is_err());
        assert!(delete_subscription(&graph, &elsewhere, &id).is_err());
        assert!(delete_subscription(&graph, &talked(), &id).is_ok());
        assert_eq!(
            bus_subscriptions(&graph, &talked())
                .expect("subscriptions")
                .as_array()
                .map(Vec::len),
            Some(0)
        );
    }

    #[test]
    fn the_trigger_endpoints_raise_priority_on_the_shipped_thresholds() {
        let graph = MemoryGraph::new();
        let sender = listener(Ok(200));
        let big = trigger_budget_variance(
            &graph,
            &talked(),
            &json!({"budget_id": "b-1", "variance_percent": 25.0, "category": "travel"}),
            &sender,
        )
        .expect("a trigger");
        let small = trigger_budget_variance(
            &graph,
            &talked(),
            &json!({"budget_id": "b-2", "variance_percent": 5.0, "category": "travel"}),
            &sender,
        )
        .expect("a trigger");
        let big_event = bus_event(&graph, &talked(), &text(&big, "event_id")).expect("an event");
        let small_event =
            bus_event(&graph, &talked(), &text(&small, "event_id")).expect("an event");
        assert_eq!(big_event["priority"], json!("high"));
        assert_eq!(small_event["priority"], json!("normal"));

        let flagged = trigger_transaction_flagged(
            &graph,
            &talked(),
            &json!({"transaction_id": "t-1", "fraud_score": 0.95, "reason": "odd"}),
            &sender,
        )
        .expect("a trigger");
        let scored = bus_event(&graph, &talked(), &text(&flagged, "event_id")).expect("an event");
        assert_eq!(scored["priority"], json!("critical"));

        // and the metrics count what the Book has published, by type
        let counted = metrics(&graph, &talked()).expect("metrics");
        assert_eq!(counted["total_events"], json!(3));
        assert_eq!(
            counted["event_counts_by_type"]["finance.budget.variance_alert"],
            json!(2)
        );
        assert_eq!(counted["broker_connected"], json!(false));
        // the types the bus can carry are listed, and an event filtered by type is the only one answered
        assert_eq!(
            event_types().as_array().map(Vec::len),
            Some(EVENT_TYPES.len())
        );
        let only = bus_events(
            &graph,
            &talked(),
            &json!({"event_type": "fraud.transaction.flagged"}),
        )
        .expect("events");
        assert_eq!(only.as_array().map(Vec::len), Some(1));
    }
}
