#![forbid(unsafe_code)]
//! webhook-service: the endpoints a Book subscribes, the events dispatched to them, and how each delivery went.
//!
//! Behaviours kept as shipped, each pinned by a test:
//!
//! * an endpoint is registered for a company with a URL, the events it wants, and a secret. It is active until it is
//!   not, and registering answers with its id, its URL and the events it asked for - not the secret;
//! * the endpoints an event goes to are the caller's own, for the company named, switched on, and subscribed to that
//!   event. An endpoint that wants other events is left alone;
//! * dispatching writes ONE delivery for each endpoint it goes to, whether or not the call reached anyone. A delivery
//!   that was answered under 400 is delivered; an answer of 400 or over is failed; one that could not be reached at all
//!   is failed too, and no reason is invented for it. Each delivery remembers how many attempts it took (one), the code
//!   it got, and when it last tried;
//! * a signed endpoint is signed: the signature is an HMAC-SHA256 over the body as it was sent, in the header the
//!   shipped service used, and an endpoint with no secret is sent unsigned rather than signed over nothing;
//! * a listing of deliveries is newest first and answers at most the number asked for, fifty by default.
//!
//! Reaching the outside is not this crate's business. A [`Sender`] carries the request, so the service can be checked
//! without a network while prod still delivers for real.

use std::collections::BTreeMap;

use hmac::{Hmac, Mac};
use serde_json::{json, Value};
use sha2::Sha256;

use vimbai_store::read::{field, flag, optional, refused, text, whole};
use vimbai_store::{BookScope, Graph, RecordKind, Records, StoreError};

/// A subscribed endpoint.
pub const ENDPOINT: RecordKind = RecordKind::new("WebhookEndpoint", "OWNS_WEBHOOK_ENDPOINT");

/// One attempt at telling an endpoint about an event.
pub const DELIVERY: RecordKind = RecordKind::new("WebhookDelivery", "OWNS_WEBHOOK_DELIVERY");

type HmacSha256 = Hmac<Sha256>;

/// Whoever carries a webhook request to its URL and brings back the code it was answered with.
pub trait Sender {
    /// Post `body` to `url` with these headers, and answer the status code, or why it could not be reached.
    fn post(&self, url: &str, body: &str, headers: &[(String, String)]) -> Result<u16, String>;
}

/// A sender that never reaches anyone, for services that only record what they would have sent.
pub struct NoSender;

impl Sender for NoSender {
    fn post(&self, _url: &str, _body: &str, _headers: &[(String, String)]) -> Result<u16, String> {
        Err("no sender is configured".to_string())
    }
}

/// The signature of a body, as the shipped service computed it: HMAC-SHA256 over the exact text sent.
pub fn signature(secret: &str, body: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).expect("any key length is allowed");
    mac.update(body.as_bytes());
    let bytes = mac.finalize().into_bytes();
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// The events an endpoint asked for, given as a list or as the comma-separated form a query string carries.
fn events_of(payload: &Value, key: &str) -> Vec<String> {
    match payload.get(key) {
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

fn endpoint_answer(record: &Value) -> Value {
    json!({
        "id": field(record, "id"),
        "company_id": field(record, "company_id"),
        "url": field(record, "url"),
        "events": events_of(record, "events"),
        "active": flag(record, "active"),
        "created_at": field(record, "created_at"),
    })
}

fn delivery_answer(record: &Value) -> Value {
    json!({
        "id": field(record, "id"),
        "endpoint_id": field(record, "endpoint_id"),
        "event_type": field(record, "event_type"),
        "payload": record.get("payload").cloned().unwrap_or(Value::Null),
        "status": field(record, "status"),
        "attempts": whole(record, "attempts"),
        "response_code": whole(record, "response_code"),
        "last_error": optional(record, "last_error"),
        "last_attempt": optional(record, "last_attempt"),
    })
}

/// `POST /endpoints`: subscribe an endpoint to events.
pub fn register_endpoint(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let company_id = text(payload, "company_id");
    if company_id.is_empty() {
        return Err(refused("company_id is required"));
    }
    let url = text(payload, "url");
    if url.is_empty() {
        return Err(refused("url is required"));
    }
    let active = match payload.get("active") {
        None | Some(Value::Null) => true,
        Some(_) => flag(payload, "active"),
    };
    let created = Records::new(graph, ENDPOINT).create(
        scope,
        &json!({
            "id": vimbai_common::ids::service_id(),
            "company_id": company_id,
            "url": url,
            "secret": text(payload, "secret"),
            "events": events_of(payload, "events"),
            "active": active,
            "created_at": text(payload, "created_at"),
        }),
    )?;
    // the answer is the id, the url and the events - the secret is never read back
    Ok(json!({
        "id": field(&created, "id"),
        "url": field(&created, "url"),
        "events": events_of(&created, "events"),
    }))
}

/// `GET /endpoints/{company_id}`: a company's endpoints, the caller's own.
pub fn endpoints_of(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    company_id: &str,
) -> Result<Value, StoreError> {
    let held = Records::new(graph, ENDPOINT).list(scope, 1000)?;
    let mine: Vec<Value> = held
        .iter()
        .filter(|endpoint| field(endpoint, "company_id") == company_id)
        .map(endpoint_answer)
        .collect();
    Ok(Value::Array(mine))
}

/// `POST /dispatch/{company_id}`: tell every subscribed endpoint about an event, and record how each went.
pub fn dispatch_event(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    sender: &dyn Sender,
    company_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let event_type = text(payload, "event_type");
    if event_type.is_empty() {
        return Err(refused("event_type is required"));
    }
    let body_payload = payload.get("payload").cloned().unwrap_or_else(|| json!({}));
    // the body is the payload as text, so the signature covers exactly what was sent
    let body = body_payload.to_string();
    let held = Records::new(graph, ENDPOINT).list(scope, 1000)?;
    let subscribed: Vec<Value> = held
        .iter()
        .filter(|endpoint| field(endpoint, "company_id") == company_id)
        .filter(|endpoint| flag(endpoint, "active"))
        .filter(|endpoint| {
            events_of(endpoint, "events")
                .iter()
                .any(|held| held == &event_type)
        })
        .cloned()
        .collect();

    let when = text(payload, "now");
    let mut deliveries = Vec::new();
    for endpoint in &subscribed {
        let mut headers = vec![
            ("Content-Type".to_string(), "application/json".to_string()),
            ("X-Event-Type".to_string(), event_type.clone()),
        ];
        let secret = field(endpoint, "secret");
        // an endpoint with no secret is sent unsigned rather than signed over nothing
        if !secret.is_empty() {
            headers.push(("X-Signature-256".to_string(), signature(secret, &body)));
        }
        // a call that never landed is failed, and the reason it gave is kept rather than invented
        let (status, code, why) = match sender.post(field(endpoint, "url"), &body, &headers) {
            Ok(code) if code < 400 => ("delivered", code, Value::Null),
            Ok(code) => ("failed", code, Value::Null),
            Err(reason) => ("failed", 0, json!(reason)),
        };
        let delivery = Records::new(graph, DELIVERY).create(
            scope,
            &json!({
                "id": vimbai_common::ids::service_id(),
                "endpoint_id": field(endpoint, "id"),
                "event_type": event_type,
                "payload": body_payload.clone(),
                "status": status,
                "attempts": 1,
                "response_code": code,
                "last_error": why,
                "last_attempt": when,
            }),
        )?;
        deliveries.push(delivery_answer(&delivery));
    }
    let successful = deliveries
        .iter()
        .filter(|delivery| delivery["status"] == json!("delivered"))
        .count();
    Ok(json!({
        "company_id": company_id,
        "event_type": event_type,
        "deliveries": deliveries,
        "total_sent": deliveries.len(),
        "successful": successful,
    }))
}

/// `GET /deliveries/{endpoint_id}`: how the tells to one endpoint went, newest first.
pub fn deliveries_of(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    endpoint_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let limit = {
        let asked = whole(payload, "limit") as usize;
        if asked == 0 {
            50
        } else {
            asked
        }
    };
    let mut held: Vec<Value> = Records::new(graph, DELIVERY)
        .list(scope, 1000)?
        .into_iter()
        .filter(|delivery| field(delivery, "endpoint_id") == endpoint_id)
        .collect();
    held.sort_by(|left, right| field(right, "last_attempt").cmp(field(left, "last_attempt")));
    held.truncate(limit);
    Ok(Value::Array(held.iter().map(delivery_answer).collect()))
}

/// The counts a listing of deliveries reads as, by status, when somebody asks for a summary rather than the rows.
pub fn delivery_totals(rows: &[Value]) -> BTreeMap<String, i64> {
    let mut totals = BTreeMap::new();
    for row in rows {
        *totals.entry(field(row, "status").to_string()).or_insert(0) += 1;
    }
    totals
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use vimbai_store::fake::MemoryGraph;

    /// One request as it was carried: the address, the body, and the headers it went with.
    type Carried = (String, String, Vec<(String, String)>);

    /// A sender that answers whatever it is told to, and remembers what it was asked to carry.
    struct Told(Result<u16, String>, RefCell<Vec<Carried>>);

    impl Told {
        fn answering(code: u16) -> Self {
            Self(Ok(code), RefCell::new(Vec::new()))
        }

        fn unreachable() -> Self {
            Self(
                Err("connection refused".to_string()),
                RefCell::new(Vec::new()),
            )
        }

        fn carried(&self) -> std::cell::Ref<'_, Vec<Carried>> {
            self.1.borrow()
        }
    }

    impl Sender for Told {
        fn post(&self, url: &str, body: &str, headers: &[(String, String)]) -> Result<u16, String> {
            self.1
                .borrow_mut()
                .push((url.to_string(), body.to_string(), headers.to_vec()));
            self.0.clone()
        }
    }

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn an_endpoint(graph: &MemoryGraph, events: Value, secret: &str, active: bool) -> Value {
        register_endpoint(
            graph,
            &scope(),
            &json!({
                "company_id": "co-1",
                "url": "https://hooks.example.test/invoices",
                "secret": secret,
                "events": events,
                "active": active,
                "created_at": "2026-01-01T09:00:00Z",
            }),
        )
        .expect("an endpoint")
    }

    #[test]
    fn an_endpoint_is_registered_for_the_events_it_asked_for() {
        let graph = MemoryGraph::new();
        let made = an_endpoint(&graph, json!(["invoice.created"]), "s3cret", true);
        // the answer never carries the secret back
        assert!(made.get("secret").is_none());
        assert_eq!(made["events"], json!(["invoice.created"]));

        let listed = endpoints_of(&graph, &scope(), "co-1").expect("endpoints");
        assert_eq!(listed.as_array().map(|held| held.len()), Some(1));
        assert_eq!(listed[0]["active"], json!(true));
        // another company's endpoints are not this company's
        let other = endpoints_of(&graph, &scope(), "co-2").expect("endpoints");
        assert_eq!(other.as_array().map(|held| held.len()), Some(0));
        // and a registration with no URL is refused
        assert!(register_endpoint(&graph, &scope(), &json!({"company_id": "co-1"})).is_err());
    }

    #[test]
    fn a_dispatch_goes_only_to_the_endpoints_that_want_that_event() {
        let graph = MemoryGraph::new();
        an_endpoint(&graph, json!(["invoice.created"]), "", true);
        an_endpoint(&graph, json!(["payment.received"]), "", true);
        an_endpoint(&graph, json!(["invoice.created"]), "", false);
        let sender = Told::answering(200);
        let sent = dispatch_event(
            &graph,
            &scope(),
            &sender,
            "co-1",
            &json!({"event_type": "invoice.created", "payload": {"id": "INV-1"},
                    "now": "2026-01-02T09:00:00Z"}),
        )
        .expect("a dispatch");
        assert_eq!(sent["total_sent"], json!(1));
        assert_eq!(sent["successful"], json!(1));
        // the body is the payload as text, and the event is named in a header
        let carried = sender.carried();
        assert_eq!(carried.len(), 1);
        assert_eq!(carried[0].1, json!({"id": "INV-1"}).to_string());
        assert!(carried[0]
            .2
            .iter()
            .any(|(name, value)| name == "X-Event-Type" && value == "invoice.created"));
    }

    #[test]
    fn a_signed_endpoint_is_signed_and_an_unsigned_one_is_not() {
        let graph = MemoryGraph::new();
        an_endpoint(&graph, json!(["invoice.created"]), "s3cret", true);
        let sender = Told::answering(201);
        dispatch_event(
            &graph,
            &scope(),
            &sender,
            "co-1",
            &json!({"event_type": "invoice.created", "payload": {"id": "INV-1"},
                    "now": "2026-01-02T09:00:00Z"}),
        )
        .expect("a dispatch");
        let carried = sender.carried();
        let signature_header = carried[0]
            .2
            .iter()
            .find(|(name, _)| name == "X-Signature-256")
            .map(|(_, value)| value.clone())
            .expect("a signature");
        assert_eq!(signature_header, signature("s3cret", &carried[0].1));
        assert_eq!(signature_header.len(), 64);

        // an endpoint with no secret is sent unsigned rather than signed over nothing
        let graph = MemoryGraph::new();
        an_endpoint(&graph, json!(["invoice.created"]), "", true);
        let plain = Told::answering(200);
        dispatch_event(
            &graph,
            &scope(),
            &plain,
            "co-1",
            &json!({"event_type": "invoice.created", "payload": {}, "now": "2026-01-02T09:00:00Z"}),
        )
        .expect("a dispatch");
        assert!(!plain.carried()[0]
            .2
            .iter()
            .any(|(name, _)| name == "X-Signature-256"));
    }

    #[test]
    fn a_delivery_is_recorded_whether_or_not_it_reached_anyone() {
        let graph = MemoryGraph::new();
        let made = an_endpoint(&graph, json!(["invoice.created"]), "", true);
        let endpoint_id = made["id"].as_str().expect("an id").to_string();

        let refused = Told::answering(500);
        let failed = dispatch_event(
            &graph,
            &scope(),
            &refused,
            "co-1",
            &json!({"event_type": "invoice.created", "payload": {},
                    "now": "2026-01-02T09:00:00Z"}),
        )
        .expect("a dispatch");
        assert_eq!(failed["successful"], json!(0));
        assert_eq!(failed["deliveries"][0]["status"], json!("failed"));
        assert_eq!(failed["deliveries"][0]["response_code"], json!(500));

        let nowhere = Told::unreachable();
        let missed = dispatch_event(
            &graph,
            &scope(),
            &nowhere,
            "co-1",
            &json!({"event_type": "invoice.created", "payload": {},
                    "now": "2026-01-02T10:00:00Z"}),
        )
        .expect("a dispatch");
        // a call that never landed is failed, and no code is invented for it
        assert_eq!(missed["deliveries"][0]["status"], json!("failed"));
        assert_eq!(missed["deliveries"][0]["response_code"], json!(0));
        assert_eq!(missed["deliveries"][0]["attempts"], json!(1));

        let listed = deliveries_of(&graph, &scope(), &endpoint_id, &json!({"limit": 1}))
            .expect("deliveries");
        assert_eq!(listed.as_array().map(|held| held.len()), Some(1));
        // newest first
        assert_eq!(listed[0]["last_attempt"], json!("2026-01-02T10:00:00Z"));
        let all = deliveries_of(&graph, &scope(), &endpoint_id, &json!({})).expect("deliveries");
        assert_eq!(
            delivery_totals(all.as_array().expect("rows")),
            BTreeMap::from([("failed".to_string(), 2)])
        );
    }
}
