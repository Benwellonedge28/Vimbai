#![forbid(unsafe_code)]
//! The money a document says it was raised in, and the rate it was raised at.
//!
//! A Book keeps one set of books in one money, but a customer may be invoiced in another. Every part of the system that
//! reads a document's money reads it here, so the same document is understood the same way by the invoicing register and
//! by the ledger, and a rate that cannot be used is refused with one wording rather than two.
use serde_json::Value;

use crate::money::Money;

/// The rate a document states, or nothing when it states none.
///
/// A rate of nothing or less is refused here rather than at each caller: a document carried at a rate that is not a
/// rate would post figures nobody can defend.
pub fn exchange_rate(currency: &str, value: Option<&Value>) -> Result<Option<Money>, String> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let stated = value
        .as_str()
        .map(|text| text.trim().to_string())
        .or_else(|| value.as_f64().map(|number| number.to_string()))
        .unwrap_or_default();
    if stated.is_empty() {
        return Ok(None);
    }
    let rate = Money::parse(&stated).map_err(|_| {
        format!("The exchange rate must be a figure, as how much one {currency} is worth; it was {stated}.")
    })?;
    if rate.is_zero() || rate.is_negative() {
        return Err(format!(
            "The exchange rate from {currency} must be more than nothing; it was {}.",
            rate.to_fixed(6).trim_end_matches('0').trim_end_matches('.')
        ));
    }
    Ok(Some(rate))
}

/// The money a document names, in the shape a Book compares it in: trimmed and upper-cased, or nothing when unnamed.
pub fn named_currency(value: Option<&Value>) -> String {
    value
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .trim()
        .to_uppercase()
}
