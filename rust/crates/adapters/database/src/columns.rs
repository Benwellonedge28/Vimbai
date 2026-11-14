//! The columns every store keeps beside a record's own fields.
//!
//! Both file-backed stores index the same four things - the kind, the id, the Book that owns the row and the person it
//! concerns - and both have to guard a property name before it goes anywhere near a query. Keeping that in one place means
//! a Postgres row and a SQLite row cannot disagree about what an indexed column holds.

use serde_json::Value;

use crate::StoreError;

/// The columns a record is indexed by, read out of the record itself so a caller never supplies them separately.
pub(crate) fn indexed(props: &Value) -> (Option<String>, Option<String>, Option<String>) {
    let text = |name: &str| props.get(name).and_then(Value::as_str).map(str::to_owned);
    (text("book_id"), text("user_id"), text("created_at"))
}

pub(crate) fn unquote(text: &str) -> Result<Value, StoreError> {
    serde_json::from_str(text)
        .map_err(|problem| StoreError::Graph(format!("a stored record is unreadable: {problem}")))
}

pub(crate) fn id_of(props: &Value) -> Result<&str, StoreError> {
    props
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| StoreError::Invalid("a stored record needs an id".into()))
}

/// A monotonic-ish stamp for ordering. The wall clock in microseconds, which is enough to keep a Book's entries in the
/// order they were written even when two land in the same millisecond.
pub(crate) fn now_seq() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_micros() as i64)
        .unwrap_or(0)
}

/// A property name is a column the code holds, never a caller's text. Anything else would be a statement built by
/// concatenation, which is the mistake this whole design exists to avoid.
pub(crate) fn checked_property(property: &str) -> Result<&str, StoreError> {
    if property.is_empty()
        || !property
            .chars()
            .all(|letter| letter.is_ascii_alphanumeric() || letter == '_')
    {
        return Err(StoreError::Invalid(format!(
            "'{property}' is not a property name"
        )));
    }
    Ok(property)
}
