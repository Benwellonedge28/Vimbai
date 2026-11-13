#![forbid(unsafe_code)]
//! The small figures every budgeting service reaches for, in one place.
//!
//! Six of these were copied into each service as it was ported: a one and a hundred to turn rates into arithmetic, a
//! reader that turns a payload field into money, a reader for a rate that may be absent, a two-place rendering for an
//! answer, and a two-place rounding of a plain number. Copies drift, so they live here once and every service in this
//! crate imports them.

use serde_json::{json, Value};

use vimbai_common::{py_round, Money};
use vimbai_store::read::{decimal, refused};
use vimbai_store::StoreError;

/// One, for turning a rate into a multiplier.
pub(crate) fn one() -> Money {
    Money::parse("1").unwrap_or_else(|_| Money::zero())
}

/// A hundred, for turning a rate into a percentage.
pub(crate) fn hundred() -> Money {
    Money::parse("100").unwrap_or_else(|_| Money::zero())
}

/// A payload field read as money, refusing anything that is not an amount.
pub(crate) fn amount(payload: &Value, key: &str, fallback: &str) -> Result<Money, StoreError> {
    Money::parse(&decimal(payload, key, fallback))
        .map_err(|_| refused(&format!("{key} must be an amount")))
}

/// A payload field read as a rate, where an absent or unreadable figure counts as nothing.
pub(crate) fn rate(payload: &Value, key: &str) -> Money {
    Money::parse(&decimal(payload, key, "0")).unwrap_or_else(|_| Money::zero())
}

/// An answer figure: money rounded to two places and given as its digits.
pub(crate) fn rounded(value: &Money) -> Value {
    json!(value.round(2).to_fixed(2))
}

/// A plain number rounded the way the services rounded it.
pub(crate) fn r2(value: f64) -> f64 {
    py_round(value, 2)
}

/// A plain number read as money.
pub(crate) fn money(value: f64) -> Money {
    Money::from_f64_shortest(value).unwrap_or_else(|_| Money::zero())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_amount_that_is_not_a_number_is_refused() {
        assert!(amount(&json!({"total": "a lot"}), "total", "0").is_err());
        assert_eq!(
            amount(&json!({"total": "1200"}), "total", "0")
                .unwrap()
                .to_fixed(2),
            "1200.00"
        );
    }

    #[test]
    fn a_rate_that_is_absent_counts_as_nothing() {
        assert!(rate(&json!({}), "growth").is_zero());
        assert_eq!(
            rate(&json!({"growth": 0.05}), "growth").to_fixed(4),
            "0.0500"
        );
    }

    #[test]
    fn an_answer_figure_keeps_two_places() {
        let value = Money::parse("1234.5678").unwrap();
        assert_eq!(rounded(&value), json!("1234.57"));
    }

    #[test]
    fn a_hundred_and_a_one_are_what_the_arithmetic_expects() {
        assert_eq!(one().to_fixed(0), "1");
        assert_eq!(hundred().to_fixed(0), "100");
        assert_eq!(money(2.5).to_fixed(1), "2.5");
        assert_eq!(r2(2.345), 2.35);
    }
}
