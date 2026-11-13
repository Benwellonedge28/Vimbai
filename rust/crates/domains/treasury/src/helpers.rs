#![forbid(unsafe_code)]
//! Treasury's own narrow additions, on top of the readers and writers every service shares.
//!
//! The readers themselves now live once in `vimbai_store::read` and the writers in `vimbai_store::write`, because
//! every domain had its own copy of them. What is left here is only what treasury needs and nothing else does, and
//! it is re-exported under the names the modules already use so there is still one place to look.

use serde_json::Value;

pub(crate) use vimbai_store::read::{
    field, money, money_or_nothing, optional, or_default, refused, text, whole,
};
pub(crate) use vimbai_store::write::{remember, store_record};

/// A figure that is not an amount of money: a holding period, a count, a z-score input.
pub(crate) fn number(payload: &Value, key: &str) -> Option<f64> {
    match payload.get(key) {
        Some(Value::Number(value)) => value.as_f64(),
        Some(Value::String(value)) => value.trim().parse().ok(),
        _ => None,
    }
}

/// A rate or a ratio: the shared reader, under the name the treasury modules already call it.
pub(crate) use vimbai_store::read::decimal as percent;
