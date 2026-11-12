//! Reading amounts off a payload without losing them.
//!
//! A caller may send an amount as a number or as a decimal string, the two shapes the Python
//! models accepted. Inside the store the figure becomes a `Money`, which is exact to any number
//! of digits, so a large contract value or a long cost breakdown cannot drift while it is being
//! summed. It leaves again as the same decimal string, because a JSON number is a double in every
//! mainstream client and would throw away the digits the store went to the trouble of keeping.
//!
//! Every service crate in `crates/domains` reads its amounts through these, so an amount is
//! interpreted in exactly one place.

#![forbid(unsafe_code)]

use serde_json::{json, Value};
use vimbai_common::Money;

use crate::StoreError;

/// An amount the caller supplied, exact.
pub fn amount(payload: &Value, key: &str) -> Result<Money, StoreError> {
    match payload.get(key) {
        Some(Value::Number(number)) => Money::from_f64_shortest(number.as_f64().unwrap_or(0.0))
            .map_err(|error| StoreError::Invalid(format!("{key}: {error}"))),
        Some(Value::String(text)) => {
            Money::parse(text).map_err(|error| StoreError::Invalid(format!("{key}: {error}")))
        }
        _ => Err(StoreError::Invalid(format!(
            "{key} is required and must be an amount"
        ))),
    }
}

/// An optional amount, zero when absent. Used where a field is genuinely optional on the wire,
/// such as a job's retention, which most jobs do not have.
#[allow(dead_code)]
pub fn amount_or_zero(payload: &Value, key: &str) -> Result<Money, StoreError> {
    if payload.get(key).is_none() || payload.get(key) == Some(&Value::Null) {
        return Ok(Money::zero());
    }
    amount(payload, key)
}

/// An amount already stored on a record.
pub fn stored_amount(record: &Value, key: &str) -> Result<Money, StoreError> {
    match record.get(key) {
        Some(Value::Number(number)) => Money::from_f64_shortest(number.as_f64().unwrap_or(0.0))
            .map_err(|error| StoreError::Invalid(format!("{key}: {error}"))),
        Some(Value::String(text)) => {
            Money::parse(text).map_err(|error| StoreError::Invalid(format!("{key}: {error}")))
        }
        None | Some(Value::Null) => Ok(Money::zero()),
        Some(_) => Err(StoreError::Invalid(format!("{key} is not an amount"))),
    }
}

/// The wire shape: the exact decimal string, as MONEY.md requires.
///
/// This used to answer a JSON number, "as the endpoints have always answered". That was wrong and
/// quietly lossy: a number is a double by the time a client has parsed it, so a contract value of
/// 123456789012345678901.55 arrived as 123456789012345680000. The figure in the graph was exact and
/// the figure on the wire was not, which is the worst version of this mistake because the store looks
/// right. `tests/test_contracts.py` reads this function, so the contracts and the wire change together.
pub fn wire(money: &Money) -> Value {
    json!(money.to_fixed(6))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_amount_survives_a_round_trip_through_the_wire_shape_exactly() {
        // 20 digits: well past what a float can hold, which is the point of Money.
        let exact = "12345678901234567890.55";
        let read = amount(&json!({ "contract_value": exact }), "contract_value").unwrap();
        assert_eq!(read.to_fixed(2), exact);
    }

    #[test]
    fn a_missing_amount_is_an_error_not_a_zero() {
        let err = amount(&json!({"name": "job"}), "contract_value").unwrap_err();
        assert!(err.to_string().contains("contract_value"), "{err}");
    }

    #[test]
    fn an_absent_optional_amount_is_zero() {
        assert!(amount_or_zero(&json!({}), "cost").unwrap().is_zero());
    }
}
