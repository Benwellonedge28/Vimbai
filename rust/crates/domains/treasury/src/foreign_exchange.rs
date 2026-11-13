#![forbid(unsafe_code)]
//! foreign-exchange-service: the spot table, a conversion between two currencies, and what an exposure costs
//! when it is only partly hedged.
//!
//! Nothing is stored: every answer is worked out from the request and the table. Rates are kept as exact
//! decimal text rather than floats, so a conversion of a large amount does not drift in the last cent, and are
//! published as decimal text for the same reason. A rate is not an amount of money, so it is rounded to six
//! places; an amount is rounded to two.
//!
//! Behaviours kept as shipped, each pinned by a test:
//!
//! * the table is the service's own, base USD, and it is the only source of rates;
//! * asking for a currency that is not in the table is a miss on `/rates/{currency}` and a refusal on
//!   `/convert` and `/hedge`, which is how the original answered;
//! * a hedge with no forward rate falls back to the spot rate, and its potential loss is what the unhedged
//!   part would move by, expressed per hundred units of the spot rate rather than per unit, which is how the
//!   original published it;
//! * the hedge cost is 0.2% of the hedged part;
//! * a hedge ratio above one would hedge money that is not exposed, so it is refused rather than accepted.

use serde_json::{json, Value};
use vimbai_common::Money;
use vimbai_store::StoreError;

use crate::helpers::{percent, refused};

/// A fixed figure, kept as exact decimal text.
fn exact(text: &str) -> Money {
    Money::parse(text).unwrap_or_else(|_| Money::zero())
}

/// The service's spot table, base USD, as exact decimal text.
const SPOT: &[(&str, &str)] = &[
    ("USD", "1"),
    ("EUR", "0.92"),
    ("GBP", "0.79"),
    ("ZWL", "15.5"),
    ("ZAR", "18.5"),
    ("BWP", "13.6"),
    ("KES", "152"),
    ("NGN", "1600"),
    ("GHS", "15.2"),
    ("CNY", "7.25"),
    ("JPY", "149.5"),
];

/// What one unit of `currency` is worth in the base currency, or nothing when it is not in the table.
fn spot(currency: &str) -> Option<Money> {
    SPOT.iter()
        .find(|(code, _)| *code == currency)
        .and_then(|(_, rate)| Money::parse(rate).ok())
}

fn unsupported() -> StoreError {
    refused("Unsupported currency")
}

/// `GET /rates`: the whole table.
pub fn rates() -> Value {
    let mut table = serde_json::Map::new();
    for (code, rate) in SPOT {
        let value = Money::parse(rate).unwrap_or_else(|_| Money::zero());
        table.insert((*code).to_string(), json!(value.to_fixed(6)));
    }
    json!({ "base": "USD", "rates": Value::Object(table) })
}

/// `GET /rates/{currency}`.
pub fn rate_for(currency: &str) -> Result<Value, StoreError> {
    let rate = spot(currency).ok_or(StoreError::NotFound)?;
    Ok(json!({
        "currency": currency,
        "rate": rate.to_fixed(6),
        "base": "USD",
    }))
}

/// `POST /convert`: how much of `to_currency` the amount is worth.
pub fn convert(payload: &Value) -> Result<Value, StoreError> {
    let from = crate::helpers::text(payload, "from_currency");
    let to = crate::helpers::text(payload, "to_currency");
    let amount = crate::helpers::money(payload, "amount")?;

    let (Some(from_spot), Some(to_spot)) = (spot(&from), spot(&to)) else {
        return Err(unsupported());
    };
    if from_spot.is_zero() {
        return Err(refused("Unsupported currency"));
    }

    let rate = to_spot.over(&from_spot);
    let converted = amount.times(&rate);

    Ok(json!({
        "from_currency": from,
        "to_currency": to,
        "amount": amount.round(2).to_fixed(2),
        "rate": rate.round(6).to_fixed(6),
        "converted_amount": converted.round(2).to_fixed(2),
        "rate_date": crate::helpers::text(payload, "rate_date"),
    }))
}

/// `POST /hedge`: what part of an exposure is covered, and what the uncovered part is exposed to.
pub fn hedge_exposure(payload: &Value) -> Result<Value, StoreError> {
    let currency = crate::helpers::text(payload, "currency");
    let spot_rate = spot(&currency).ok_or_else(unsupported)?;
    let exposure = crate::helpers::money(payload, "exposure_amount")?;

    let ratio = Money::parse(&percent(payload, "hedge_ratio", "1"))
        .map_err(|_| refused("hedge_ratio must be a ratio between 0 and 1"))?;
    if ratio.is_negative() || ratio.as_decimal() > exact("1").as_decimal() {
        return Err(refused("hedge_ratio must be a ratio between 0 and 1"));
    }

    let hedged = exposure.times(&ratio);
    let unhedged = exposure.minus(&hedged);
    // no forward rate means the exposure is measured at spot, which leaves the rate risk at zero
    let forward = match crate::helpers::percent(payload, "forward_rate", "") {
        text if text.is_empty() => spot_rate.clone(),
        text => Money::parse(&text).map_err(|_| refused("forward_rate must be a rate"))?,
    };

    let move_pct = exact("100");
    let potential_loss = unhedged
        .times(&forward.minus(&spot_rate).abs())
        .over(&spot_rate)
        .times(&move_pct);
    // 0.2% of the hedged part: what covering it typically costs
    let hedge_cost = hedged.times(&exact("0.002"));

    Ok(json!({
        "company_id": crate::helpers::text(payload, "company_id"),
        "currency": currency,
        "exposure_amount": exposure.round(2).to_fixed(2),
        "hedged_amount": hedged.round(2).to_fixed(2),
        "unhedged_amount": unhedged.round(2).to_fixed(2),
        "hedge_ratio": ratio.round(6).to_fixed(6),
        "forward_rate": forward.round(6).to_fixed(6),
        "potential_loss_unhedged": potential_loss.round(2).to_fixed(2),
        "hedge_cost_estimate": hedge_cost.round(2).to_fixed(2),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_is_base_usd_and_the_base_is_one() {
        let table = rates();
        assert_eq!(table["base"], json!("USD"));
        assert_eq!(table["rates"]["USD"], json!("1.000000"));
        assert!(table["rates"].as_object().unwrap().len() >= 11);
    }

    #[test]
    fn converting_between_two_currencies_crosses_through_the_base() {
        // 100 EUR into ZAR: 100 / 0.92 * 18.5
        let answer = convert(&json!({
            "from_currency": "EUR", "to_currency": "ZAR", "amount": "100", "rate_date": "2026-02-01"
        }))
        .expect("converted");
        assert_eq!(answer["converted_amount"], json!("2010.87"));
        assert_eq!(answer["rate"], json!("20.108696"));
        assert_eq!(answer["amount"], json!("100.00"));
        assert_eq!(answer["rate_date"], json!("2026-02-01"));
    }

    #[test]
    fn a_large_amount_does_not_lose_its_last_cent() {
        // 123456789012345.67 USD into USD is itself, exactly
        let answer = convert(&json!({
            "from_currency": "USD", "to_currency": "USD", "amount": "123456789012345.67"
        }))
        .expect("converted");
        assert_eq!(answer["converted_amount"], json!("123456789012345.67"));
    }

    #[test]
    fn a_currency_outside_the_table_is_a_miss_or_a_refusal() {
        assert!(matches!(
            rate_for("XXX"),
            Err(vimbai_store::StoreError::NotFound)
        ));
        assert!(convert(&json!({
            "from_currency": "USD", "to_currency": "XXX", "amount": "1"
        }))
        .is_err());
    }

    #[test]
    fn a_hedge_measures_what_the_uncovered_part_is_exposed_to() {
        // 1000 ZAR exposed, half hedged, forward 19 against spot 18.5: the uncovered 500 moves 0.5/18.5
        let answer = hedge_exposure(&json!({
            "company_id": "c-1", "currency": "ZAR", "exposure_amount": "1000",
            "hedge_ratio": "0.5", "forward_rate": "19"
        }))
        .expect("hedged");
        assert_eq!(answer["hedged_amount"], json!("500.00"));
        assert_eq!(answer["unhedged_amount"], json!("500.00"));
        // as shipped: the move per hundred units, not per unit
        assert_eq!(answer["potential_loss_unhedged"], json!("1351.35"));
        assert_eq!(answer["hedge_cost_estimate"], json!("1.00"));
    }

    #[test]
    fn a_hedge_with_no_forward_rate_is_measured_at_spot_and_owes_no_rate_loss() {
        let answer = hedge_exposure(&json!({
            "company_id": "c-1", "currency": "EUR", "exposure_amount": "100"
        }))
        .expect("hedged");
        assert_eq!(answer["forward_rate"], json!("0.920000"));
        assert_eq!(answer["potential_loss_unhedged"], json!("0.00"));
        assert_eq!(answer["hedged_amount"], json!("100.00"));
    }

    #[test]
    fn a_hedge_ratio_above_one_is_refused() {
        assert!(hedge_exposure(&json!({
            "company_id": "c-1", "currency": "EUR", "exposure_amount": "100", "hedge_ratio": "1.5"
        }))
        .is_err());
    }
}
