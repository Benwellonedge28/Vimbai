#![forbid(unsafe_code)]
//! The readers and writers every treasury module was carrying a copy of.
//!
//! Each service arrived as its own container, and each one re-implemented the same six little functions for
//! reading a field, defaulting a label and remembering a record. They were identical, so they are one set here
//! rather than five copies that could drift apart the first time one of them was fixed.

use serde_json::{json, Value};

use vimbai_common::Money;
use vimbai_store::amounts::amount;
use vimbai_store::{created_record, BookScope, Graph, Records, StoreError};

/// A refusal the caller can act on.
pub(crate) fn refused(message: &str) -> StoreError {
    StoreError::Invalid(message.to_string())
}

/// A label, empty when it was never given.
pub(crate) fn text(payload: &Value, key: &str) -> String {
    payload
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// A label, with the service's own default when it was never given.
pub(crate) fn or_default(payload: &Value, key: &str, fallback: &str) -> String {
    match text(payload, key) {
        value if value.is_empty() => fallback.to_string(),
        value => value,
    }
}

/// A label that may be absent, for a filter that only narrows when it is there.
pub(crate) fn optional(payload: &Value, key: &str) -> Option<String> {
    match payload.get(key).and_then(Value::as_str) {
        Some(value) if !value.is_empty() => Some(value.to_string()),
        _ => None,
    }
}

/// A stored label.
pub(crate) fn field<'a>(record: &'a Value, key: &str) -> &'a str {
    record.get(key).and_then(Value::as_str).unwrap_or("")
}

/// An amount the caller must supply.
pub(crate) fn money(payload: &Value, key: &str) -> Result<Money, StoreError> {
    match payload.get(key) {
        Some(Value::Null) | None => Err(refused(&format!("{key} is required"))),
        Some(value) => amount(&json!({ "amount": value }), "amount"),
    }
}

/// An amount that counts as nothing when it was never given.
pub(crate) fn money_or_nothing(payload: &Value, key: &str) -> Result<Money, StoreError> {
    match payload.get(key) {
        Some(Value::Null) | None => Ok(Money::zero()),
        Some(value) => amount(&json!({ "amount": value }), "amount"),
    }
}

/// A figure that is not an amount of money: a holding period, a count, a z-score input.
pub(crate) fn number(payload: &Value, key: &str) -> Option<f64> {
    match payload.get(key) {
        Some(Value::Number(value)) => value.as_f64(),
        Some(Value::String(value)) => value.trim().parse().ok(),
        _ => None,
    }
}

/// A rate or a ratio, kept as its exact decimal text rather than a float.
pub(crate) fn percent(payload: &Value, key: &str, fallback: &str) -> String {
    match payload.get(key) {
        Some(Value::Number(value)) => value.to_string(),
        Some(Value::String(value)) if !value.is_empty() => value.clone(),
        _ => fallback.to_string(),
    }
}

/// Write a record and answer with it, so the caller sees what was stored rather than what was sent.
pub(crate) fn remember(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    kind: vimbai_store::RecordKind,
    body: Value,
) -> Result<Value, StoreError> {
    let saved = Records::new(graph, kind).create(scope, &body)?;
    let mut answer = created_record(&saved);
    if let (Some(answer), Some(fields)) = (answer.as_object_mut(), body.as_object()) {
        for (key, value) in fields {
            answer.entry(key.clone()).or_insert_with(|| value.clone());
        }
    }
    Ok(answer)
}
