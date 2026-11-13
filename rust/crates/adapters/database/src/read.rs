#![forbid(unsafe_code)]
//! The readers every service was carrying its own copy of.
//!
//! A ported service takes a JSON request and reads stored records back out of the graph, so nearly all of them
//! need the same handful of readers: a label that may be absent, a label with a default, a filter that only
//! narrows when it is given, an amount the caller must supply and an amount that counts as nothing. Each module
//! arrived with its own copy, and the copies had already begun to differ in ways that were not deliberate. They
//! live here once.

use serde_json::{json, Value};

use vimbai_common::Money;

use crate::amounts::amount;
use crate::StoreError;

/// A refusal the caller can act on.
pub fn refused(message: &str) -> StoreError {
    StoreError::Invalid(message.to_string())
}

/// A yes/no field on a stored record: only a stored `true` counts as yes.
pub fn flag(record: &Value, key: &str) -> bool {
    record.get(key).and_then(Value::as_bool).unwrap_or(false)
}

/// A yes/no field that may not have been set at all.
pub fn maybe_flag(record: &Value, key: &str) -> Option<bool> {
    record.get(key).and_then(Value::as_bool)
}

/// A label, empty when it was never given.
pub fn text(payload: &Value, key: &str) -> String {
    payload
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// A label the caller must supply.
pub fn required<'a>(payload: &'a Value, key: &str) -> Result<&'a str, StoreError> {
    payload
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| refused(&format!("{key} is required")))
}

/// A label, with the service's own default when it was never given.
pub fn or_default(payload: &Value, key: &str, fallback: &str) -> String {
    match text(payload, key) {
        value if value.is_empty() => fallback.to_string(),
        value => value,
    }
}

/// A label that is there when it was sent, even when it is empty, and absent when it was not sent.
pub fn present(payload: &Value, key: &str) -> Option<String> {
    payload.get(key).and_then(Value::as_str).map(str::to_string)
}

/// A label that may arrive as text or as a bare number.
///
/// Query strings are read as JSON, so `?bank_account=1234` reaches a service as a number. Reading it with
/// this keeps an account number written either way.
pub fn label(payload: &Value, key: &str) -> String {
    match payload.get(key) {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Number(number)) => number.to_string(),
        Some(Value::Bool(flag)) => flag.to_string(),
        _ => String::new(),
    }
}

/// Which of the spellings the services used for a field was actually sent, so the caller can read that one.
pub fn key_under<'a>(payload: &'a Value, names: &[&'a str]) -> Option<&'a str> {
    names
        .iter()
        .copied()
        .find(|name| payload.get(*name).is_some_and(|value| !value.is_null()))
}

/// A label that must be there and must say something, for a field that is the whole point of the request.
pub fn required_text(payload: &Value, key: &str) -> Result<String, StoreError> {
    let value = required(payload, key)?;
    if value.trim().is_empty() {
        return Err(refused(&format!("{key} is required")));
    }
    Ok(value.to_string())
}

/// A label that may be absent, for a filter that only narrows when it is there.
pub fn optional(payload: &Value, key: &str) -> Option<String> {
    match text(payload, key) {
        value if value.is_empty() => None,
        value => Some(value),
    }
}

/// A stored label.
pub fn field<'a>(record: &'a Value, key: &str) -> &'a str {
    record.get(key).and_then(Value::as_str).unwrap_or("")
}

/// A whole number, given as a number or as digits in a string, and nothing when it is neither.
pub fn whole(record: &Value, key: &str) -> u32 {
    match record.get(key) {
        Some(Value::Number(number)) => number.as_u64().map_or(0, |value| value as u32),
        Some(Value::String(digits)) => digits.trim().parse().unwrap_or(0),
        _ => 0,
    }
}

/// A whole number the caller must supply, refused when it is not one.
pub fn whole_signed(payload: &Value, key: &str) -> Result<i64, StoreError> {
    match payload.get(key) {
        Some(Value::Number(number)) => number
            .as_i64()
            .ok_or_else(|| refused(&format!("{key} must be a whole number"))),
        Some(Value::String(digits)) => digits
            .trim()
            .parse()
            .map_err(|_| refused(&format!("{key} must be a whole number"))),
        _ => Err(refused(&format!("{key} is required"))),
    }
}

/// An amount the caller must supply.
pub fn money(payload: &Value, key: &str) -> Result<Money, StoreError> {
    match payload.get(key) {
        Some(Value::Null) | None => Err(refused(&format!("{key} is required"))),
        Some(value) => amount(&json!({ "amount": value }), "amount"),
    }
}

/// An amount that counts as nothing when it was never given.
pub fn money_or_nothing(payload: &Value, key: &str) -> Result<Money, StoreError> {
    match payload.get(key) {
        Some(Value::Null) | None => Ok(Money::zero()),
        Some(value) => amount(&json!({ "amount": value }), "amount"),
    }
}

/// A series of amounts, read exactly. Absent means an empty series, which the callers that need one
/// refuse themselves rather than being given a zero they did not send.
pub fn amounts(payload: &Value, key: &str) -> Result<Vec<vimbai_common::Money>, StoreError> {
    let items = match payload.get(key) {
        None | Some(Value::Null) => return Ok(Vec::new()),
        Some(Value::Array(items)) => items.clone(),
        Some(_) => return Err(refused(&format!("{key} must be a list of amounts"))),
    };
    items
        .iter()
        .map(|item| match item {
            Value::String(text) => vimbai_common::Money::parse(text),
            Value::Number(number) => {
                vimbai_common::Money::from_f64_shortest(number.as_f64().unwrap_or_default())
            }
            _ => vimbai_common::Money::parse("0"),
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| refused(&format!("{key} holds an amount that cannot be read")))
}

/// A rate or a ratio, kept as its exact decimal text rather than being turned into a float on the way in.
pub fn decimal(payload: &Value, key: &str, fallback: &str) -> String {
    match payload.get(key) {
        Some(Value::Number(value)) => value.to_string(),
        Some(Value::String(value)) if !value.is_empty() => value.clone(),
        _ => fallback.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_label_that_was_never_given_reads_empty_and_one_that_was_reads_back() {
        assert_eq!(text(&json!({"a": "given"}), "a"), "given");
        assert_eq!(text(&json!({}), "a"), "");
        assert_eq!(text(&json!({"a": 7}), "a"), "", "a number is not a label");
    }

    #[test]
    fn a_required_label_says_which_one_is_missing() {
        let refusal = required(&json!({}), "company_id").expect_err("refused");
        assert_eq!(refusal.to_string(), "company_id is required");
        assert_eq!(
            required(&json!({"company_id": "c-1"}), "company_id"),
            Ok("c-1")
        );
    }

    #[test]
    fn a_whole_number_arrives_as_a_number_or_as_digits_and_never_as_a_panic() {
        assert_eq!(whole(&json!({"n": 12}), "n"), 12);
        assert_eq!(whole(&json!({"n": "12"}), "n"), 12);
        assert_eq!(whole(&json!({"n": " 12 "}), "n"), 12);
        assert_eq!(
            whole(&json!({"n": 1.5}), "n"),
            0,
            "half a term is not a term"
        );
        assert_eq!(whole(&json!({}), "n"), 0);
    }

    #[test]
    fn an_amount_that_must_be_supplied_refuses_and_one_that_may_be_absent_is_nothing() {
        assert!(money(&json!({}), "principal").is_err());
        assert!(money_or_nothing(&json!({}), "principal")
            .expect("nothing")
            .is_zero());
        assert_eq!(
            money(&json!({"principal": "10.5"}), "principal")
                .expect("amount")
                .to_fixed(2),
            "10.50"
        );
    }
}
