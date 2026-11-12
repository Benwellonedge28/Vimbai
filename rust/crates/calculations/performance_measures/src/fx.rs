//! Foreign exchange: the conversion table and a simple hedge calculation.
//!
//! The rate table is the service's own, held as units per US dollar. It is
//! deliberately a constant rather than a fetched rate: converting a ledger figure
//! must reproduce the number that was posted, not today's market.

use serde::Serialize;

use vimbai_common::numbers::{money, round_dp};

/// Units of each currency per one US dollar.
pub const RATES: [(&str, f64); 11] = [
    ("USD", 1.0),
    ("EUR", 0.92),
    ("GBP", 0.79),
    ("ZWL", 15.5),
    ("ZAR", 18.5),
    ("BWP", 13.6),
    ("KES", 152.0),
    ("NGN", 1600.0),
    ("GHS", 15.2),
    ("CNY", 7.25),
    ("JPY", 149.5),
];

pub fn rate_of(currency: &str) -> Option<f64> {
    RATES
        .iter()
        .find(|(code, _)| *code == currency)
        .map(|(_, rate)| *rate)
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConversionRequest {
    pub from_currency: String,
    pub to_currency: String,
    pub amount: f64,
    pub rate_date: String,
}

impl ConversionRequest {
    pub fn new(from_currency: &str, to_currency: &str, amount: f64) -> ConversionRequest {
        ConversionRequest {
            from_currency: from_currency.to_string(),
            to_currency: to_currency.to_string(),
            amount,
            rate_date: String::new(),
        }
    }

    pub fn on(mut self, rate_date: &str) -> ConversionRequest {
        self.rate_date = rate_date.to_string();
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ConversionResult {
    pub from_currency: String,
    pub to_currency: String,
    pub amount: f64,
    pub rate: f64,
    pub converted_amount: f64,
    pub rate_date: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnsupportedCurrency(pub String);

impl std::fmt::Display for UnsupportedCurrency {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Unsupported currency: {}", self.0)
    }
}

impl std::error::Error for UnsupportedCurrency {}

/// Convert through the table. Both currencies must be known, as before; the
/// caller turns the error into a 400.
pub fn convert(
    request: &ConversionRequest,
    rate_date: &str,
) -> Result<ConversionResult, UnsupportedCurrency> {
    let from = rate_of(&request.from_currency)
        .ok_or_else(|| UnsupportedCurrency(request.from_currency.clone()))?;
    let to = rate_of(&request.to_currency)
        .ok_or_else(|| UnsupportedCurrency(request.to_currency.clone()))?;
    let rate = to / from;
    let date = if request.rate_date.is_empty() {
        rate_date.to_string()
    } else {
        request.rate_date.clone()
    };
    Ok(ConversionResult {
        from_currency: request.from_currency.clone(),
        to_currency: request.to_currency.clone(),
        amount: money(request.amount),
        rate: round_dp(rate, 6),
        converted_amount: money(request.amount * rate),
        rate_date: date,
    })
}

#[derive(Debug, Clone, PartialEq)]
pub struct HedgeRequest {
    pub company_id: String,
    pub currency: String,
    pub exposure_amount: f64,
    pub hedge_ratio: f64,
    pub forward_rate: f64,
}

impl HedgeRequest {
    pub fn new(
        company_id: &str,
        currency: &str,
        exposure_amount: f64,
        hedge_ratio: f64,
    ) -> HedgeRequest {
        HedgeRequest {
            company_id: company_id.to_string(),
            currency: currency.to_string(),
            exposure_amount,
            hedge_ratio,
            forward_rate: 0.0,
        }
    }

    pub fn at_forward(mut self, forward_rate: f64) -> HedgeRequest {
        self.forward_rate = forward_rate;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HedgeResult {
    pub company_id: String,
    pub currency: String,
    pub exposure_amount: f64,
    pub hedged_amount: f64,
    pub unhedged_amount: f64,
    pub hedge_ratio: f64,
    pub forward_rate: f64,
    pub potential_loss_unhedged: f64,
    pub hedge_cost_estimate: f64,
}

/// Hedge a currency exposure.
///
/// Preserved quirk, and a real scale error: the rate gap is converted to a
/// percentage and then multiplied by the unhedged amount, which multiplies the
/// figure by 100. A 3.26% move on 100,000 unhedged reports 326,086.96 instead of
/// 3,260.87. Kept because hedging reports were built on it; callers should read
/// it as a relative measure until it is corrected deliberately. The 0.2% hedge
/// cost is the service's standing assumption, not a quoted price.
pub fn hedge(request: &HedgeRequest) -> Result<HedgeResult, UnsupportedCurrency> {
    let spot =
        rate_of(&request.currency).ok_or_else(|| UnsupportedCurrency(request.currency.clone()))?;
    let hedged = request.exposure_amount * request.hedge_ratio;
    let unhedged = request.exposure_amount - hedged;
    let forward = if request.forward_rate != 0.0 {
        request.forward_rate
    } else {
        spot
    };
    let potential_loss = unhedged * (forward - spot).abs() / spot * 100.0;
    let hedge_cost = hedged * 0.002;

    Ok(HedgeResult {
        company_id: request.company_id.clone(),
        currency: request.currency.clone(),
        exposure_amount: money(request.exposure_amount),
        hedged_amount: money(hedged),
        unhedged_amount: money(unhedged),
        hedge_ratio: request.hedge_ratio,
        forward_rate: round_dp(forward, 6),
        potential_loss_unhedged: money(potential_loss),
        hedge_cost_estimate: money(hedge_cost),
    })
}
