#![forbid(unsafe_code)]
//! One JSON door onto the calculation libraries, shared by both callers.
//!
//! The Python seam and the HTTP interface were each turning JSON into library types
//! and back. Keeping that in one place means a route and a seam function cannot drift
//! apart, and the arithmetic stays in the libraries either way.

pub mod assurance;
pub mod http;

pub fn floats_from_strings(value: &serde_json::Value) -> serde_json::Value {
    floats_from_strings_except(value, &[])
}

/// The same conversion, leaving named fields as text. A year such as "2026" parses as
/// a number, so a service that publishes it as a string keeps it a string.
pub fn floats_from_strings_except(
    value: &serde_json::Value,
    keep_text: &[&str],
) -> serde_json::Value {
    match value {
        serde_json::Value::String(text) => text
            .parse::<f64>()
            .map(serde_json::Value::from)
            .unwrap_or_else(|_| value.clone()),
        serde_json::Value::Array(items) => serde_json::Value::Array(
            items
                .iter()
                .map(|item| floats_from_strings_except(item, keep_text))
                .collect(),
        ),
        serde_json::Value::Object(map) => serde_json::Value::Object(
            map.iter()
                .map(|(k, v)| {
                    if keep_text.contains(&k.as_str()) {
                        (k.clone(), v.clone())
                    } else {
                        (k.clone(), floats_from_strings_except(v, keep_text))
                    }
                })
                .collect(),
        ),
        other => other.clone(),
    }
}
