#![forbid(unsafe_code)]
//! pos-integration-service: the tills a Book runs, and the takings they report.
//!
//! A till is registered once and answers to the id the outside world knows it by. Takings arrive against that id and are
//! held as records, each with what it was (a sale, a refund, a void, an adjustment, a layaway, a return), how it was paid,
//! and where it stands with the ledger. The sales summary is those records added up.
//!
//! The service's own rules, all kept:
//!
//! * a till is registered OFFLINE, because a new registration has never spoken to anything yet;
//! * the same till registered twice, or the same till's transaction id received twice, is a conflict rather than a second
//!   record, so a till replaying its queue after a dropped connection cannot double the takings;
//! * a device status change is NOT validated - the shipped endpoint took a plain map and stored whatever it was given, so
//!   a state nobody agreed on is stored as given. That is shipped behaviour and a test pins it;
//! * reconciling inventory reports nothing found, always. The shipped loop compared nothing at all, so it could only ever
//!   answer zero discrepancies; the port keeps that answer rather than inventing comparisons that were never there.
//!
//! What the port deliberately does NOT carry: the shipped service "processed transactions for accounting" in a background
//! task that minted a journal reference `JE-xxxxxxxx` and updated the row, while never posting anything. The ledger has one
//! route in and corrections are reversals, so this crate records a taking as pending and, when a caller marks it synced,
//! records that without inventing a journal id for an entry that does not exist. `POST /transactions/{id}/sync` is the one
//! door added, and it exists so that state is reachable at all now that nothing runs in the background.
//!
//! Nor does it carry the two WebSocket doors (`/ws/pos/{id}`, `/ws/dashboard`): a socket is a transport, and every
//! connection in this platform is served by the one websocket service rather than by the service that happened to open it.

use serde_json::{json, Value};

use vimbai_common::Money;
use vimbai_store::read::{money_or_nothing, text};
use vimbai_store::{BookScope, Graph, RecordKind, Records, StoreError};

/// A till a Book runs.
pub const DEVICE: RecordKind = RecordKind::new("PosDevice", "OWNS_POS_DEVICE");
/// A taking reported by a till.
pub const TAKING: RecordKind = RecordKind::new("PosTransaction", "OWNS_POS_TRANSACTION");

/// The states a till can report itself in.
pub const DEVICE_STATES: [&str; 4] = ["online", "offline", "syncing", "error"];
/// The six things a taking can be.
pub const TAKING_KINDS: [&str; 6] = ["sale", "refund", "void", "adjustment", "layaway", "return"];
/// The six ways a customer can pay.
pub const PAYMENT_METHODS: [&str; 6] = ["cash", "card", "mobile", "split", "gift_card", "loyalty"];
/// Where a taking stands with the ledger.
pub const SYNC_STATES: [&str; 4] = ["pending", "synced", "failed", "partial"];

fn moment(payload: &Value) -> Value {
    match payload.get("now") {
        Some(stated) if !stated.is_null() => stated.clone(),
        _ => json!(vimbai_common::clock::moment()),
    }
}

fn one_of(named: &str, allowed: &[&str], what: &str) -> Result<String, StoreError> {
    if allowed.contains(&named) {
        return Ok(named.to_string());
    }
    Err(StoreError::Invalid(format!(
        "{what} must be one of: {} (got {named})",
        allowed.join(", ")
    )))
}

/// When something happened, as a moment a span can be compared against.
///
/// The engine stamps its own moments as seconds, and a caller states a bound as text; both are read here so a report asked
/// for an afternoon measures hours rather than whole days.
fn instant(held: &Value, key: &str) -> Option<f64> {
    match held.get(key) {
        Some(Value::Number(seconds)) => seconds.as_f64(),
        Some(Value::String(written)) => vimbai_common::dates::instant_of(written),
        _ => None,
    }
}

fn found(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    kind: RecordKind,
    field: &str,
    wanted: &str,
) -> Result<Option<Value>, StoreError> {
    Ok(Records::new(graph, kind)
        .list(scope, 5000)?
        .into_iter()
        .find(|row| text(row, field) == wanted))
}

fn require(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    kind: RecordKind,
    field: &str,
    wanted: &str,
) -> Result<Value, StoreError> {
    found(graph, scope, kind, field, wanted)?.ok_or(StoreError::NotFound)
}

/// `POST /devices`: register a till.
pub fn pos_register_device(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let device_id = text(payload, "device_id");
    if device_id.is_empty() {
        return Err(StoreError::Invalid("device_id is required".into()));
    }
    let name = text(payload, "device_name");
    if name.chars().count() < 3 || name.chars().count() > 100 {
        return Err(StoreError::Invalid(
            "device_name must be between 3 and 100 characters".into(),
        ));
    }
    if found(graph, scope, DEVICE, "device_id", &device_id)?.is_some() {
        return Err(StoreError::Conflict("Device already registered".into()));
    }
    let now = moment(payload);
    Records::new(graph, DEVICE).create(
        scope,
        &json!({
            "device_id": device_id,
            "device_name": name,
            "device_type": text(payload, "device_type"),
            "location_id": text(payload, "location_id"),
            "api_key": text(payload, "api_key"),
            "webhook_url": text(payload, "webhook_url"),
            "enabled": payload.get("enabled").cloned().unwrap_or(json!(true)),
            // a till that has just been registered has never spoken to anything
            "status": "offline",
            "last_sync": Value::Null,
            "created_at": now,
            "updated_at": now,
        }),
    )
}

/// `GET /devices`: the tills this Book runs, optionally only those in one state.
pub fn pos_devices(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let asked = text(payload, "status");
    let wanted = if asked.is_empty() {
        None
    } else {
        Some(one_of(&asked, &DEVICE_STATES, "status")?)
    };
    let rows = Records::new(graph, DEVICE)
        .list(scope, 5000)?
        .into_iter()
        .filter(|row| {
            wanted
                .as_deref()
                .is_none_or(|state| text(row, "status") == state)
        })
        .collect();
    Ok(Value::Array(rows))
}

/// `GET /devices/{id}`: one till, by the id it answers to.
pub fn pos_device(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    device_id: &str,
) -> Result<Value, StoreError> {
    require(graph, scope, DEVICE, "device_id", device_id)
}

/// `PUT /devices/{id}/status`: record the state a till reports.
///
/// The state is not validated, because the shipped endpoint took a plain map and stored whatever arrived. A test pins
/// that so nobody "fixes" it into a refusal by accident.
pub fn pos_set_status(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    device_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let held = pos_device(graph, scope, device_id)?;
    let asked = text(payload, "status");
    let state = if asked.is_empty() {
        "online".to_string()
    } else {
        asked
    };
    let now = moment(payload);
    Records::new(graph, DEVICE).update(
        scope,
        &text(&held, "id"),
        &json!({"status": state, "last_sync": now, "updated_at": now}),
    )?;
    Ok(json!({"status": "updated", "device_id": device_id}))
}

fn validate_taking(payload: &Value) -> Result<(), StoreError> {
    if text(payload, "transaction_id").is_empty() {
        return Err(StoreError::Invalid("transaction_id is required".into()));
    }
    if text(payload, "device_id").is_empty() {
        return Err(StoreError::Invalid("device_id is required".into()));
    }
    one_of(
        &text(payload, "transaction_type"),
        &TAKING_KINDS,
        "transaction_type",
    )?;
    one_of(
        &text(payload, "payment_method"),
        &PAYMENT_METHODS,
        "payment_method",
    )?;
    let total = money_or_nothing(payload, "total_amount")?;
    if total.is_zero() || total.is_negative() {
        return Err(StoreError::Invalid(
            "total_amount must be greater than zero".into(),
        ));
    }
    Ok(())
}

/// `POST /transactions`: take one till's report.
pub fn pos_ingest(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    validate_taking(payload)?;
    let external = text(payload, "transaction_id");
    if found(graph, scope, TAKING, "transaction_id", &external)?.is_some() {
        return Err(StoreError::Conflict("Transaction already received".into()));
    }
    let now = moment(payload);
    Records::new(graph, TAKING).create(
        scope,
        &json!({
            "transaction_id": external,
            "device_id": text(payload, "device_id"),
            "transaction_type": text(payload, "transaction_type"),
            // what a till took is money, kept to the cent as exact decimals however it arrived
            "total_amount": money_or_nothing(payload, "total_amount")?.round(2).to_fixed(2),
            "tax_amount": money_or_nothing(payload, "tax_amount")?.round(2).to_fixed(2),
            "discount_amount": money_or_nothing(payload, "discount_amount")?.round(2).to_fixed(2),
            "payment_method": text(payload, "payment_method"),
            "payment_details": payload.get("payment_details").cloned().unwrap_or(Value::Null),
            "items": payload.get("items").cloned().unwrap_or_else(|| json!([])),
            "customer_id": text(payload, "customer_id"),
            "employee_id": text(payload, "employee_id"),
            "location_id": text(payload, "location_id"),
            "notes": text(payload, "notes"),
            "timestamp": match payload.get("timestamp") {
                Some(stated) if !stated.is_null() => stated.clone(),
                _ => now.clone(),
            },
            // nothing here posts to the ledger, so a taking arrives pending and says so rather than pretending
            "sync_status": "pending",
            "journal_entry_id": Value::Null,
            "processed_at": Value::Null,
            "error_message": Value::Null,
            "created_at": now,
        }),
    )
}

/// `POST /transactions/batch`: a till emptying its queue, answered one line at a time.
pub fn pos_batch(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let queued = payload.as_array().cloned().unwrap_or_default();
    let mut results = Vec::with_capacity(queued.len());
    for report in &queued {
        let external = text(report, "transaction_id");
        match pos_ingest(graph, scope, report) {
            Ok(saved) => results.push(json!({
                "transaction_id": external,
                "status": "accepted",
                "internal_id": text(&saved, "id"),
            })),
            // one refused report must not stop the ones behind it
            Err(error) => results.push(json!({
                "transaction_id": external,
                "status": "rejected",
                "reason": error.to_string(),
            })),
        }
    }
    Ok(json!({"total": queued.len(), "results": results}))
}

/// `GET /transactions`: takings, newest first, with the filters the service offered.
pub fn pos_transactions(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let device = text(payload, "device_id");
    let asked = text(payload, "sync_status");
    let wanted = if asked.is_empty() {
        None
    } else {
        Some(one_of(&asked, &SYNC_STATES, "sync_status")?)
    };
    let from = instant(payload, "start_date");
    let to = instant(payload, "end_date");
    let limit = match payload.get("limit") {
        Some(_) => (vimbai_store::read::number(payload, "limit").round() as i64).clamp(1, 500),
        None => 100,
    } as usize;

    let mut rows: Vec<Value> = Records::new(graph, TAKING)
        .list(scope, 5000)?
        .into_iter()
        .filter(|row| device.is_empty() || text(row, "device_id") == device)
        .filter(|row| {
            wanted
                .as_deref()
                .is_none_or(|state| text(row, "sync_status") == state)
        })
        .filter(|row| {
            from.is_none_or(|bound| instant(row, "timestamp").is_some_and(|when| when >= bound))
        })
        .filter(|row| {
            to.is_none_or(|bound| instant(row, "timestamp").is_some_and(|when| when <= bound))
        })
        .collect();
    // newest first, by the moment rather than by how the moment is written
    rows.sort_by(|one, two| {
        instant(two, "created_at")
            .unwrap_or(0.0)
            .total_cmp(&instant(one, "created_at").unwrap_or(0.0))
    });
    rows.truncate(limit);
    Ok(Value::Array(rows))
}

/// `GET /transactions/{id}`: one taking, by the id its till knows it by.
pub fn pos_transaction(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    transaction_id: &str,
) -> Result<Value, StoreError> {
    require(graph, scope, TAKING, "transaction_id", transaction_id)
}

/// `POST /transactions/{id}/sync`: record that a taking has reached the ledger.
///
/// No journal id is minted. The shipped service answered with `JE-xxxxxxxx` for an entry it never posted, and a
/// reference to nothing is worse than none, so the row is marked synced and the caller is told the ledger posting is not
/// this service's to make.
pub fn pos_mark_synced(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    transaction_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let held = pos_transaction(graph, scope, transaction_id)?;
    let now = moment(payload);
    let state = match text(payload, "sync_status").as_str() {
        "" => "synced".to_string(),
        asked => one_of(asked, &SYNC_STATES, "sync_status")?,
    };
    let updated = Records::new(graph, TAKING).update(
        scope,
        &text(&held, "id"),
        &json!({
            "sync_status": state,
            "processed_at": now,
            "error_message": text(payload, "error_message"),
            "journal_entry_id": Value::Null,
        }),
    )?;
    let mut answer = updated;
    answer["journal_posted"] = json!(false);
    answer["ledger_note"] =
        json!("the ledger is posted through its own door; this service records the taking only");
    Ok(answer)
}

/// `POST /inventory/sync`: what the tills say their shelves hold.
///
/// Nothing is written, because there is no stock ledger here to write to; the shipped endpoint reported the same and
/// updated a supply-chain service that does not exist. The count is reported, and `recorded` says plainly that no stock
/// moved.
pub fn pos_inventory_sync(payload: &Value) -> Result<Value, StoreError> {
    let products = payload
        .get("products")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    Ok(json!({
        "status": "synced",
        "device_id": text(payload, "device_id"),
        "items_updated": products,
        "recorded": false,
        "timestamp": moment(payload),
    }))
}

/// `POST /inventory/reconcile`: what the tills claim against what the platform holds.
///
/// The shipped loop compared nothing, so it could only ever answer that nothing was found. Kept, and pinned, rather than
/// inventing a comparison the service never made.
pub fn pos_reconcile_inventory(payload: &Value) -> Result<Value, StoreError> {
    let items = payload
        .get("inventory_data")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    Ok(json!({
        "device_id": text(payload, "device_id"),
        "total_items": items,
        "discrepancies_found": 0,
        "discrepancies": [],
    }))
}

/// `POST /reports/sales-summary`: what one till took over a span, added up.
pub fn pos_sales_summary(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let device = text(payload, "device_id");
    let from = instant(payload, "start_date");
    let to = instant(payload, "end_date");

    let rows: Vec<Value> = Records::new(graph, TAKING)
        .list(scope, 5000)?
        .into_iter()
        .filter(|row| text(row, "device_id") == device)
        .filter(|row| {
            let when = instant(row, "timestamp");
            when.is_some_and(|moment| {
                from.is_none_or(|bound| moment >= bound) && to.is_none_or(|bound| moment <= bound)
            })
        })
        .collect();

    let mut sales = Money::zero();
    let mut refunds = Money::zero();
    let mut voids = Money::zero();
    let mut by_method: Vec<(String, Money)> = Vec::new();
    let mut by_type = [("sale", 0i64), ("refund", 0i64), ("void", 0i64)];

    for row in &rows {
        let amount = money_or_nothing(row, "total_amount")?;
        let kind = text(row, "transaction_type");
        match kind.as_str() {
            "sale" => sales = sales.plus(&amount),
            "refund" => refunds = refunds.plus(&amount),
            "void" => voids = voids.plus(&amount),
            _ => {}
        }
        if let Some(slot) = by_type.iter_mut().find(|(name, _)| *name == kind) {
            slot.1 += 1;
        }
        let method = text(row, "payment_method");
        match by_method.iter_mut().find(|(name, _)| *name == method) {
            Some(slot) => slot.1 = slot.1.plus(&amount),
            None => by_method.push((method, amount)),
        }
    }

    let mut methods = serde_json::Map::new();
    for (name, total) in by_method {
        methods.insert(name, json!(total.round(2).to_f64_lossy()));
    }
    Ok(json!({
        "device_id": device,
        "start_date": text(payload, "start_date"),
        "end_date": text(payload, "end_date"),
        "group_by": if text(payload, "group_by").is_empty() {
            "hour".to_string()
        } else {
            text(payload, "group_by")
        },
        "total_transactions": rows.len(),
        "total_sales": sales.round(2).to_f64_lossy(),
        "total_refunds": refunds.round(2).to_f64_lossy(),
        "total_voids": voids.round(2).to_f64_lossy(),
        "net_sales": sales.minus(&refunds).minus(&voids).round(2).to_f64_lossy(),
        "by_payment_method": Value::Object(methods),
        "by_type": {
            "sale": by_type[0].1,
            "refund": by_type[1].1,
            "void": by_type[2].1,
        },
    }))
}

/// `POST /integrations/{pos_type}/webhook`: a taking reported by an outside system.
///
/// The three transforms are kept exactly as they were, including the awkward ones: Square and Stripe state their amounts
/// in cents and are divided by a hundred, Shopify states them in money and is not, and a CentOS-named amount that is
/// missing counts as nothing rather than refusing the report. Anything that is not one of the three named systems is read
/// as a taking already in this platform's own shape.
pub fn pos_webhook(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    pos_type: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let reported = match pos_type {
        "square" => json!({
            "transaction_id": label(payload, "id"),
            "device_id": text(payload, "location_id"),
            "transaction_type": "sale",
            "total_amount": cents(payload.get("total_money"))?,
            "tax_amount": cents(payload.get("tax_money"))?,
            "discount_amount": 0,
            "payment_method": "card",
            "items": [],
        }),
        "stripe" => json!({
            "transaction_id": label(payload, "id"),
            "device_id": text(payload.get("metadata").unwrap_or(&Value::Null), "device_id"),
            "transaction_type": "sale",
            "total_amount": cents(payload.get("amount"))?,
            "tax_amount": 0,
            "discount_amount": 0,
            "payment_method": "card",
            "items": [],
            "timestamp": payload.get("created").cloned().unwrap_or(Value::Null),
        }),
        "shopify" => json!({
            "transaction_id": label(payload, "id"),
            "device_id": text(payload, "gateway"),
            "transaction_type": "sale",
            "total_amount": payload.get("total_price").cloned().unwrap_or(json!(0)),
            "tax_amount": payload.get("total_tax").cloned().unwrap_or(json!(0)),
            "discount_amount": payload.get("total_discounts").cloned().unwrap_or(json!(0)),
            "payment_method": "card",
            "items": [],
        }),
        _ => payload.clone(),
    };
    let saved = pos_ingest(graph, scope, &reported)?;
    Ok(json!({
        "status": "received",
        "transaction_id": text(&saved, "transaction_id"),
    }))
}

/// A label an outside system sent, which may be a number rather than text.
fn label(payload: &Value, key: &str) -> Value {
    match payload.get(key) {
        Some(Value::String(named)) => json!(named),
        Some(Value::Number(digits)) => json!(digits.to_string()),
        _ => json!(""),
    }
}

/// A cents figure, either as `{"amount": 1250}` or as the number itself.
fn cents(held: Option<&Value>) -> Result<Value, StoreError> {
    let inner = match held {
        Some(Value::Object(map)) => map.get("amount").cloned().unwrap_or(json!(0)),
        Some(other) => other.clone(),
        None => json!(0),
    };
    let amount = money_or_nothing(&json!({"amount": inner}), "amount")?;
    Ok(json!(amount
        .over(&Money::from_i64(100))
        .round(2)
        .to_f64_lossy()))
}

/// `GET /metrics`: what this Book's tills have taken.
pub fn pos_metrics(graph: &(impl Graph + ?Sized), scope: &BookScope) -> Result<Value, StoreError> {
    let takings = Records::new(graph, TAKING).list(scope, 5000)?;
    let devices = Records::new(graph, DEVICE).list(scope, 5000)?;
    let count_of = |state: &str| -> i64 {
        takings
            .iter()
            .filter(|row| text(row, "sync_status") == state)
            .count() as i64
    };
    let mut total = Money::zero();
    for row in &takings {
        total = total.plus(&money_or_nothing(row, "total_amount")?);
    }
    Ok(json!({
        "total_transactions": takings.len(),
        "synced": count_of("synced"),
        "pending": count_of("pending"),
        "failed": count_of("failed"),
        "total_amount_processed": total.round(2).to_f64_lossy(),
        // a socket is a transport, and this process opens none
        "connected_devices": 0,
        "registered_devices": devices.len(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::MemoryGraph;

    fn talked() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn till(graph: &MemoryGraph, id: &str) -> Value {
        pos_register_device(
            graph,
            &talked(),
            &json!({"device_id": id, "device_name": "Front till", "device_type": "terminal"}),
        )
        .expect("a till")
    }

    fn taking_of(
        graph: &MemoryGraph,
        id: &str,
        device: &str,
        kind: &str,
        amount: &str,
        when: &str,
    ) -> Value {
        pos_ingest(
            graph,
            &talked(),
            &json!({"transaction_id": id, "device_id": device, "transaction_type": kind,
                    "total_amount": amount, "payment_method": "cash", "timestamp": when}),
        )
        .expect("a taking")
    }

    #[test]
    fn a_till_is_registered_offline_and_only_once() {
        let graph = MemoryGraph::new();
        let made = till(&graph, "till-1");
        // a till that has just been registered has never spoken to anything
        assert_eq!(made["status"], json!("offline"));
        assert_eq!(made["enabled"], json!(true));

        // the same till registered twice is a conflict, not a second record
        assert!(matches!(
            pos_register_device(
                &graph,
                &talked(),
                &json!({"device_id": "till-1", "device_name": "Front till", "device_type": "terminal"})
            ),
            Err(StoreError::Conflict(_))
        ));
        // a name too short to identify a till is refused
        assert!(matches!(
            pos_register_device(
                &graph,
                &talked(),
                &json!({"device_id": "till-2", "device_name": "ab"})
            ),
            Err(StoreError::Invalid(_))
        ));
        assert!(pos_device(&graph, &talked(), "till-1").is_ok());
        assert!(pos_device(&graph, &talked(), "till-9").is_err());
    }

    #[test]
    fn a_device_state_nobody_agreed_on_is_stored_as_given() {
        let graph = MemoryGraph::new();
        till(&graph, "till-1");
        let answer = pos_set_status(&graph, &talked(), "till-1", &json!({"status": "on fire"}))
            .expect("a change");
        assert_eq!(answer, json!({"status": "updated", "device_id": "till-1"}));
        // shipped behaviour: the endpoint took a plain map and stored whatever arrived
        let held = pos_device(&graph, &talked(), "till-1").expect("a till");
        assert_eq!(held["status"], json!("on fire"));

        // an unstated state is the one a till is assumed to be in when it speaks
        pos_set_status(&graph, &talked(), "till-1", &json!({})).expect("a change");
        assert_eq!(
            pos_device(&graph, &talked(), "till-1").expect("a till")["status"],
            json!("online")
        );

        // the listing can be narrowed to one state, and a state that is not one is refused
        assert_eq!(
            pos_devices(&graph, &talked(), &json!({"status": "online"}))
                .expect("tills")
                .as_array()
                .map(Vec::len),
            Some(1)
        );
        assert!(pos_devices(&graph, &talked(), &json!({"status": "on fire"})).is_err());
    }

    #[test]
    fn a_replayed_taking_cannot_double_the_takings() {
        let graph = MemoryGraph::new();
        till(&graph, "till-1");
        let made = taking_of(
            &graph,
            "tx-1",
            "till-1",
            "sale",
            "41.32",
            "2026-11-13T09:00:00Z",
        );
        assert_eq!(made["sync_status"], json!("pending"));
        assert_eq!(made["items"], json!([]));

        // the same report arriving twice, as a till replaying its queue would, is a conflict
        assert!(matches!(
            pos_ingest(
                &graph,
                &talked(),
                &json!({"transaction_id": "tx-1", "device_id": "till-1", "transaction_type": "sale",
                        "total_amount": 41.32, "payment_method": "cash"})
            ),
            Err(StoreError::Conflict(_))
        ));
        // a taking of nothing, or of a kind nobody agreed on, is refused rather than stored
        assert!(pos_ingest(
            &graph,
            &talked(),
            &json!({"transaction_id": "tx-2", "device_id": "till-1",
            "transaction_type": "sale", "total_amount": 0, "payment_method": "cash"})
        )
        .is_err());
        assert!(pos_ingest(
            &graph,
            &talked(),
            &json!({"transaction_id": "tx-3", "device_id": "till-1",
            "transaction_type": "hologram", "total_amount": 5, "payment_method": "cash"})
        )
        .is_err());

        // a batch answers one line at a time, and one refusal does not stop the reports behind it
        let batch = pos_batch(
            &graph,
            &talked(),
            &json!([
                {"transaction_id": "tx-1", "device_id": "till-1", "transaction_type": "sale", "total_amount": 10,
                 "payment_method": "cash"},
                {"transaction_id": "tx-4", "device_id": "till-1", "transaction_type": "sale", "total_amount": 10,
                 "payment_method": "cash"}
            ]),
        )
        .expect("a batch");
        assert_eq!(batch["total"], json!(2));
        assert_eq!(batch["results"][0]["status"], json!("rejected"));
        assert_eq!(batch["results"][1]["status"], json!("accepted"));
    }

    #[test]
    fn the_summary_adds_the_takings_up_and_nets_the_refunds_and_voids() {
        let graph = MemoryGraph::new();
        till(&graph, "till-1");
        taking_of(
            &graph,
            "tx-1",
            "till-1",
            "sale",
            "100",
            "2026-11-13T09:00:00Z",
        );
        taking_of(
            &graph,
            "tx-2",
            "till-1",
            "sale",
            "50.50",
            "2026-11-13T10:00:00Z",
        );
        taking_of(
            &graph,
            "tx-3",
            "till-1",
            "refund",
            "20.25",
            "2026-11-13T11:00:00Z",
        );
        taking_of(
            &graph,
            "tx-4",
            "till-1",
            "void",
            "5",
            "2026-11-13T12:00:00Z",
        );
        // a taking outside the span, and one from another till, must not be counted
        taking_of(
            &graph,
            "tx-5",
            "till-1",
            "sale",
            "999",
            "2026-11-14T09:00:00Z",
        );
        taking_of(
            &graph,
            "tx-6",
            "till-2",
            "sale",
            "777",
            "2026-11-13T09:30:00Z",
        );

        let summary = pos_sales_summary(
            &graph,
            &talked(),
            &json!({"device_id": "till-1", "start_date": "2026-11-13T00:00:00Z", "end_date": "2026-11-13T23:59:59Z"}),
        )
        .expect("a summary");
        assert_eq!(summary["total_transactions"], json!(4));
        assert_eq!(summary["total_sales"], json!(150.5));
        assert_eq!(summary["total_refunds"], json!(20.25));
        assert_eq!(summary["total_voids"], json!(5.0));
        assert_eq!(summary["net_sales"], json!(125.25));
        assert_eq!(summary["by_payment_method"]["cash"], json!(175.75));
        assert_eq!(
            summary["by_type"],
            json!({"sale": 2, "refund": 1, "void": 1})
        );

        // the filters the listing offered, newest first
        let listed = pos_transactions(
            &graph,
            &talked(),
            &json!({"device_id": "till-1", "limit": 2}),
        )
        .expect("takings");
        assert_eq!(listed.as_array().map(Vec::len), Some(2));

        // marking a taking synced does not pretend the ledger was posted to
        let synced = pos_mark_synced(&graph, &talked(), "tx-1", &json!({})).expect("a change");
        assert_eq!(synced["sync_status"], json!("synced"));
        assert_eq!(synced["journal_entry_id"], Value::Null);
        assert_eq!(synced["journal_posted"], json!(false));

        let metrics = pos_metrics(&graph, &talked()).expect("metrics");
        assert_eq!(metrics["synced"], json!(1));
        assert_eq!(metrics["pending"], json!(5));
        assert_eq!(metrics["registered_devices"], json!(1));
        assert_eq!(metrics["connected_devices"], json!(0));
        assert_eq!(metrics["total_amount_processed"], json!(1951.75));
    }

    #[test]
    fn the_three_outside_systems_are_read_in_their_own_units() {
        let graph = MemoryGraph::new();
        till(&graph, "till-1");

        // square and stripe state cents, so a hundredth of the figure is the money
        let square = pos_webhook(
            &graph,
            &talked(),
            "square",
            &json!({"id": "sq-1", "location_id": "till-1", "total_money": {"amount": 4132},
                    "tax_money": {"amount": 132}}),
        )
        .expect("a taking");
        assert_eq!(square["transaction_id"], json!("sq-1"));
        let held = pos_transaction(&graph, &talked(), "sq-1").expect("a taking");
        assert_eq!(held["total_amount"], json!("41.32"));
        assert_eq!(held["tax_amount"], json!("1.32"));
        assert_eq!(held["device_id"], json!("till-1"));
        assert_eq!(held["payment_method"], json!("card"));

        let stripe = pos_webhook(
            &graph,
            &talked(),
            "stripe",
            &json!({"id": "st-1", "amount": 1000, "metadata": {"device_id": "till-1"}}),
        )
        .expect("a taking");
        assert_eq!(stripe["transaction_id"], json!("st-1"));
        assert_eq!(
            pos_transaction(&graph, &talked(), "st-1").expect("a taking")["total_amount"],
            json!("10.00")
        );

        // shopify states money, so it is taken as it stands
        pos_webhook(
            &graph,
            &talked(),
            "shopify",
            &json!({"id": 8899, "gateway": "till-1", "total_price": "25.75", "total_tax": "1.75"}),
        )
        .expect("a taking");
        let shopify = pos_transaction(&graph, &talked(), "8899").expect("a taking");
        assert_eq!(shopify["total_amount"], json!("25.75"));
        assert_eq!(shopify["device_id"], json!("till-1"));

        // a system that is not one of the three is read as a taking already in this platform's shape
        pos_webhook(
            &graph,
            &talked(),
            "my-till",
            &json!({"transaction_id": "own-1", "device_id": "till-1", "transaction_type": "sale",
                    "total_amount": 5, "payment_method": "mobile"}),
        )
        .expect("a taking");
        assert_eq!(
            pos_transaction(&graph, &talked(), "own-1").expect("a taking")["payment_method"],
            json!("mobile")
        );

        // an outside report that is not a taking at all is refused, not stored under a guess
        assert!(pos_webhook(
            &graph,
            &talked(),
            "square",
            &json!({"location_id": "till-1"})
        )
        .is_err());
    }

    #[test]
    fn inventory_reconciles_to_nothing_found_because_it_compares_nothing() {
        let graph = MemoryGraph::new();
        // the shipped loop compared nothing, so it could only ever answer that nothing was found
        let reconciled = pos_reconcile_inventory(&json!({"device_id": "till-1",
            "inventory_data": [{"sku": "A", "count": 5}, {"sku": "B", "count": 2}]}))
        .expect("a reconciliation");
        assert_eq!(reconciled["total_items"], json!(2));
        assert_eq!(reconciled["discrepancies_found"], json!(0));
        assert_eq!(reconciled["discrepancies"], json!([]));

        // and stating stock writes nothing, which the answer says rather than implying stock moved
        let synced = pos_inventory_sync(&json!({"device_id": "till-1",
            "products": [{"sku": "A", "count": 5}]}))
        .expect("a sync");
        assert_eq!(synced["items_updated"], json!(1));
        assert_eq!(synced["recorded"], json!(false));
        assert_eq!(
            pos_metrics(&graph, &talked()).expect("metrics")["total_transactions"],
            json!(0)
        );
    }
}
