#![forbid(unsafe_code)]
//! The arithmetic the partnership services share.
//!
//! Two things are worth having in one place: reading a partner's contribution out of a request or a stored record,
//! and adding a list of them up. A register that added its partners differently from the summary that reports them
//! would be worse than useless, so both go through here.

use serde_json::Value;

use vimbai_common::Money;
use vimbai_store::read::{decimal, refused};
use vimbai_store::StoreError;

/// A partner's contribution, refused by name when it is not an amount.
pub fn partner_contribution(partner: &Value) -> Result<Money, StoreError> {
    Money::parse(&decimal(partner, "contribution", "0"))
        .map_err(|_| refused("a partner's contribution must be an amount"))
}

/// The capital a list of partners puts in.
pub fn total_capital(partners: &[Value]) -> Result<Money, StoreError> {
    let mut total = Money::zero();
    for partner in partners {
        total = total.plus(&partner_contribution(partner)?);
    }
    Ok(total)
}

/// The partners a record carries, empty when it carries none.
pub fn partners_of(record: &Value) -> Vec<Value> {
    record
        .get("partners")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn contributions_add_up_exactly() {
        let partners = vec![
            json!({"contribution": "0.10"}),
            json!({"contribution": "0.20"}),
        ];
        assert_eq!(total_capital(&partners).unwrap().to_string(), "0.30");
    }

    #[test]
    fn a_contribution_that_is_not_an_amount_is_refused() {
        assert!(partner_contribution(&json!({"contribution": "much"})).is_err());
    }
}
