#![forbid(unsafe_code)]
//! Reading values out of a query string.
//!
//! Everything in a query arrives as text, so a filter or a page size read with `as_u64` on the JSON
//! object silently keeps its default instead of honouring what the caller asked for. These helpers
//! read the JSON value and fall back to the text form, so `?limit=5`, `?limit=5` from a client that
//! sends numbers and `?days=10` all mean what they say. The names say `query_` because these read a
//! query object specifically, not any JSON.

use serde_json::Value;

/// A whole number, whether it arrived as a number or as text. A value that is neither is the fallback.
pub fn query_number(query: &Value, key: &str, fallback: i64) -> i64 {
    match query.get(key) {
        Some(Value::Number(number)) => number.as_i64().unwrap_or(fallback),
        Some(Value::String(text)) => text.trim().parse().unwrap_or(fallback),
        _ => fallback,
    }
}

/// A page size: a whole number, never negative, never more than the store will answer in one go.
pub fn query_page(query: &Value, fallback: i64, most: i64) -> usize {
    query_number(query, "limit", fallback).clamp(0, most) as usize
}

/// A yes or no, whether it arrived as `true`, `"true"`, `1` or `"1"`. Absent means no opinion.
pub fn query_flag(query: &Value, key: &str) -> Option<bool> {
    match query.get(key) {
        Some(Value::Bool(value)) => Some(*value),
        Some(Value::Number(number)) => number.as_i64().map(|value| value != 0),
        Some(Value::String(text)) => match text.trim().to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" => Some(true),
            "false" | "0" | "no" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

/// The text of a query value, or nothing when it was not given.
pub fn query_text(query: &Value, key: &str) -> Option<String> {
    match query.get(key) {
        Some(Value::String(value)) if !value.is_empty() => Some(value.clone()),
        Some(Value::Number(number)) => Some(number.to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn numbers_are_read_from_text_as_well_as_from_numbers() {
        assert_eq!(query_number(&json!({"limit": "5"}), "limit", 100), 5);
        assert_eq!(query_number(&json!({"limit": 5}), "limit", 100), 5);
        assert_eq!(query_number(&json!({}), "limit", 100), 100);
        // nonsense keeps the default rather than reaching for zero
        assert_eq!(query_number(&json!({"limit": "many"}), "limit", 100), 100);
    }

    #[test]
    fn a_page_size_is_never_negative_and_never_unbounded() {
        assert_eq!(query_page(&json!({"limit": "-3"}), 100, 1000), 0);
        assert_eq!(query_page(&json!({"limit": "99999"}), 100, 1000), 1000);
        assert_eq!(query_page(&json!({}), 30, 1000), 30);
    }

    #[test]
    fn a_flag_is_understood_in_every_shape_a_client_sends_it() {
        assert_eq!(
            query_flag(&json!({"reconciled": "true"}), "reconciled"),
            Some(true)
        );
        assert_eq!(
            query_flag(&json!({"reconciled": "false"}), "reconciled"),
            Some(false)
        );
        assert_eq!(
            query_flag(&json!({"reconciled": 1}), "reconciled"),
            Some(true)
        );
        assert_eq!(
            query_flag(&json!({"reconciled": 0}), "reconciled"),
            Some(false)
        );
        assert_eq!(
            query_flag(&json!({"reconciled": true}), "reconciled"),
            Some(true)
        );
        // absent is no opinion, which is not the same as no
        assert_eq!(query_flag(&json!({}), "reconciled"), None);
        assert_eq!(
            query_flag(&json!({"reconciled": "maybe"}), "reconciled"),
            None
        );
    }
}
