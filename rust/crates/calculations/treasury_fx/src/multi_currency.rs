//! The multi-currency service: a fixed rate table, a USD-only functional
//! currency, and placeholder hedge terms.
use serde::{Deserialize, Serialize};

use crate::fx_rates::round_half_even;

/// The hard-coded table from the source, keyed as concatenated pairs.
pub fn fixed_pair_rate(pair: &str) -> Option<f64> {
    let rate = match pair {
        "EURUSD" => 1.10,
        "GBPEUR" => 1.17,
        "USDJPY" => 149.5,
        "USDEUR" => 0.91,
        _ => return None,
    };
    Some(rate)
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CurrencyConversionRequest {
    pub company_id: String,
    pub amount: f64,
    pub from_currency: String,
    pub to_currency: String,
    pub conversion_date: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CurrencyConversionResponse {
    pub company_id: String,
    pub original_amount: f64,
    pub original_currency: String,
    pub converted_amount: f64,
    pub target_currency: String,
    pub exchange_rate: f64,
    pub spread: f64,
    pub converted_at: String,
}

/// `/convert`.
///
/// PYTHON QUIRK: an unknown pair converts at a rate of 1.0, and the 0.2%
/// spread is subtracted from the rate, so the amount is multiplied by
/// 0.998. The result is NOT rounded to the target currency.
pub fn convert_fixed(
    req: &CurrencyConversionRequest,
    converted_at: &str,
) -> CurrencyConversionResponse {
    let pair = format!("{}{}", req.from_currency, req.to_currency);
    let reverse = format!("{}{}", req.to_currency, req.from_currency);
    let rate = if let Some(r) = fixed_pair_rate(&pair) {
        r
    } else if let Some(r) = fixed_pair_rate(&reverse) {
        1.0 / r
    } else {
        1.0
    };
    let spread = rate * 0.002;

    CurrencyConversionResponse {
        company_id: req.company_id.clone(),
        original_amount: req.amount,
        original_currency: req.from_currency.clone(),
        converted_amount: req.amount * (rate - spread),
        target_currency: req.to_currency.clone(),
        exchange_rate: rate,
        spread,
        converted_at: converted_at.to_string(),
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TransactionRequest {
    pub company_id: String,
    pub transaction_id: String,
    pub amount: f64,
    pub currency: String,
    pub transaction_type: String,
    pub counterparty_currency: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TransactionResponse {
    pub transaction_id: String,
    pub amount_in_functional: f64,
    pub functional_currency: String,
    pub fx_gain_loss: f64,
    pub exchange_rate_used: f64,
    pub booking_status: String,
}

/// `/transaction`.
///
/// PYTHON QUIRKS, preserved: every transaction is booked in USD at a rate
/// of 1.0 whatever its currency, and the FX gain or loss is a flat 1% of
/// the amount whenever the counterparty currency is set and is not USD,
/// with a USD counterparty producing nothing.
pub fn process_transaction(req: &TransactionRequest) -> TransactionResponse {
    let fx_gain_loss = match &req.counterparty_currency {
        Some(ccy) if ccy != "USD" => req.amount * 0.01,
        _ => 0.0,
    };

    TransactionResponse {
        transaction_id: req.transaction_id.clone(),
        amount_in_functional: round_half_even(req.amount, 2),
        functional_currency: "USD".to_string(),
        fx_gain_loss: round_half_even(fx_gain_loss, 2),
        exchange_rate_used: 1.0,
        booking_status: "posted".to_string(),
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HedgeRequest {
    pub company_id: String,
    pub exposure_amount: f64,
    pub currency_pair: String,
    pub hedge_type: String,
    pub tenor: i32,
    pub notional: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HedgeResponse {
    pub hedge_id: String,
    pub instrument_type: String,
    pub strike_rate: f64,
    pub premium: f64,
    pub effectiveness_ratio: f64,
    pub mark_to_market: f64,
}

/// `/hedge`.
///
/// PYTHON QUIRKS, preserved: the strike is always 1.10, effectiveness is
/// always 0.95, the premium is 2% of the notional and the mark to market is
/// 1.5% of the exposure. The currency pair, hedge type and tenor are
/// accepted and ignored.
pub fn create_hedge(req: &HedgeRequest, hedge_id: &str) -> HedgeResponse {
    HedgeResponse {
        hedge_id: hedge_id.to_string(),
        instrument_type: req.hedge_type.clone(),
        strike_rate: 1.10,
        premium: round_half_even(req.notional * 0.02, 2),
        effectiveness_ratio: 0.95,
        mark_to_market: round_half_even(req.exposure_amount * 0.015, 2),
    }
}
