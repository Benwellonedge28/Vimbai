#![forbid(unsafe_code)]
//! What both provisions share: a rate is a percentage of an outstanding amount, worked exactly.

use serde_json::Value;

use vimbai_common::Money;
use vimbai_store::read::{decimal, field};
use vimbai_store::StoreError;

/// The share of `outstanding` that a rate of `percent` covers, to the cent.
///
/// Both provisions store a percentage (25 for a quarter) rather than a fraction, and both apply it the
/// same way, so it is applied once here. The result is rounded to two places because money is: a
/// provision of 5% against 10.03 is 0.50, not half a fraction of a cent.
pub fn share_of(outstanding: &Money, percent: &Money) -> Money {
    // the percentage is spelled as the services spell it (25, not 0.25), so it is divided by a
    // hundred exactly rather than by a float hundredth
    outstanding
        .times(percent)
        .over(&Money::from_i64(100))
        .round(2)
}

/// The rate a rule ladder gives for an age in days.
///
/// The ladder is keyed by age in days as text (`{"30": 1, "60": 5}`), because that is the shape the
/// policies were stored in. A rung the ladder does not name falls back to `fallback`, which is what the
/// shipped service did rather than treating a missing rate as no provision at all.
pub fn rung(rules: &Value, days: u32, fallback: &str) -> Money {
    let rate = rules
        .get(days.to_string())
        .map(|value| match value {
            Value::String(text) => text.clone(),
            other => other.to_string(),
        })
        .unwrap_or_else(|| fallback.to_owned());
    Money::parse(&rate).unwrap_or_else(|_| Money::zero())
}

/// The stored rate of a record, falling back when the record carries none.
pub fn stored_rate(record: &Value, key: &str, fallback: &str) -> Result<Money, StoreError> {
    Money::parse(&decimal(record, key, fallback))
        .map_err(|_| StoreError::Invalid(format!("{key} is not an amount")))
}

/// The name a record is known by, for the notes the provisions write.
pub fn name_of(record: &Value) -> String {
    let name = field(record, "debtor_name");
    if name.is_empty() {
        field(record, "debtor_id").to_owned()
    } else {
        name.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_rate_is_applied_once_and_rounded_to_the_cent() {
        let outstanding = Money::parse("10.03").expect("parsed");
        let rate = Money::parse("5").expect("parsed");
        assert_eq!(share_of(&outstanding, &rate).to_fixed(2), "0.50");
    }

    #[test]
    fn a_rung_the_ladder_does_not_name_falls_back() {
        let rules = json!({"180": 50, "90": 10});
        assert_eq!(rung(&rules, 180, "50").to_fixed(2), "50.00");
        assert_eq!(rung(&rules, 60, "5").to_fixed(2), "5.00");
    }

    #[test]
    fn a_missing_rate_is_a_zero_rather_than_a_refusal() {
        let record = json!({"debtor_name": "Ana"});
        assert!(stored_rate(&record, "provision_percent", "0")
            .expect("read")
            .is_zero());
        assert_eq!(name_of(&record), "Ana");
        assert_eq!(name_of(&json!({"debtor_id": "d-1"})), "d-1");
    }
}
