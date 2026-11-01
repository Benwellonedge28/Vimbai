//! IAS 21 foreign currency translation of a subsidiary's financial
//! statements into the presentation currency.
use serde::{Deserialize, Serialize};

use crate::fx_rates::round_half_even;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BalanceSheetItem {
    pub account: String,
    pub amount: f64,
    pub is_monetary: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TranslationRequest {
    pub company_id: String,
    pub subsidiary: String,
    pub functional_currency: String,
    pub presentation_currency: String,
    pub closing_rate: f64,
    pub avg_rate: f64,
    pub historical_rate: f64,
    pub net_assets: Vec<BalanceSheetItem>,
    pub income_statement: Vec<BalanceSheetItem>,
    pub goodwill: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TranslationDetail {
    pub account: String,
    pub original: f64,
    pub rate_used: f64,
    pub rate_type: String,
    pub translated: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TranslationResult {
    pub company_id: String,
    pub subsidiary: String,
    pub functional_currency: String,
    pub presentation_currency: String,
    pub translated_net_assets: f64,
    pub translated_income: f64,
    pub cumulative_translation_adjustment: f64,
    pub goodwill_translated: f64,
    pub exchange_differences: f64,
    pub details: Vec<TranslationDetail>,
}

/// `/translate`.
///
/// PYTHON QUIRKS, both preserved:
/// - the cumulative translation adjustment (and the "exchange differences"
///   that duplicate it) compares only MONETARY net assets, summing them at
///   the closing rate less the same sum at the AVERAGE rate. The rate that
///   non-monetary assets were actually translated at never enters the
///   calculation, and the two reported figures are always identical;
/// - every line is rounded to two places, then the totals are formed from
///   the UNROUNDED translations and rounded separately, so the reported
///   total need not equal the sum of the details.
pub fn translate_financials(req: &TranslationRequest) -> TranslationResult {
    let mut translated_assets = 0.0;
    let mut translated_income = 0.0;
    let mut details = Vec::new();

    for item in &req.net_assets {
        let rate = if item.is_monetary {
            req.closing_rate
        } else {
            req.historical_rate
        };
        let translated = item.amount * rate;
        translated_assets += translated;
        details.push(TranslationDetail {
            account: item.account.clone(),
            original: round_half_even(item.amount, 2),
            rate_used: rate,
            rate_type: if item.is_monetary {
                "closing".to_string()
            } else {
                "historical".to_string()
            },
            translated: round_half_even(translated, 2),
        });
    }

    for item in &req.income_statement {
        let translated = item.amount * req.avg_rate;
        translated_income += translated;
        details.push(TranslationDetail {
            account: item.account.clone(),
            original: round_half_even(item.amount, 2),
            rate_used: req.avg_rate,
            rate_type: "average".to_string(),
            translated: round_half_even(translated, 2),
        });
    }

    let goodwill_translated = req.goodwill * req.closing_rate;
    let monetary_total: f64 = req
        .net_assets
        .iter()
        .filter(|i| i.is_monetary)
        .map(|i| i.amount)
        .sum();
    let exchange_diff = monetary_total * req.closing_rate - monetary_total * req.avg_rate;

    TranslationResult {
        company_id: req.company_id.clone(),
        subsidiary: req.subsidiary.clone(),
        functional_currency: req.functional_currency.clone(),
        presentation_currency: req.presentation_currency.clone(),
        translated_net_assets: round_half_even(translated_assets, 2),
        translated_income: round_half_even(translated_income, 2),
        cumulative_translation_adjustment: round_half_even(exchange_diff, 2),
        goodwill_translated: round_half_even(goodwill_translated, 2),
        exchange_differences: round_half_even(exchange_diff, 2),
        details,
    }
}
