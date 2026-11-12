//! tax-calculation-service `/calculate/capital-gains`: 20% long-term
//! (>365 days), 35% short-term; losses floored at zero gain.

use serde::{Deserialize, Serialize};
use vimbai_common::py_round2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapitalGainsResult {
    pub company_id: String,
    pub asset_name: String,
    pub gain: f64,
    pub is_long_term: bool,
    pub rate: f64,
    pub tax_owed: f64,
    pub fiscal_year: i64,
}

pub fn capital_gains_tax(
    company_id: &str,
    asset_name: &str,
    purchase_price: f64,
    sale_price: f64,
    holding_period_days: i64,
    fiscal_year: i64,
) -> CapitalGainsResult {
    let gain = (sale_price - purchase_price).max(0.0);
    let is_long_term = holding_period_days > 365;
    let rate = if is_long_term { 0.20 } else { 0.35 };

    CapitalGainsResult {
        company_id: company_id.to_string(),
        asset_name: asset_name.to_string(),
        gain,
        is_long_term,
        rate,
        tax_owed: py_round2(gain * rate),
        fiscal_year,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_term_twenty_percent() {
        let r = capital_gains_tax("c1", "property", 100_000.0, 150_000.0, 400, 2026);
        assert!(r.is_long_term);
        assert_eq!(r.gain, 50_000.0);
        assert_eq!(r.tax_owed, 10_000.0);
    }

    #[test]
    fn exactly_365_days_is_short_term() {
        let r = capital_gains_tax("c1", "shares", 100_000.0, 150_000.0, 365, 2026);
        assert!(!r.is_long_term);
        assert_eq!(r.tax_owed, 17_500.0);
    }

    #[test]
    fn loss_produces_zero_gain_and_zero_tax() {
        let r = capital_gains_tax("c1", "shares", 100_000.0, 80_000.0, 500, 2026);
        assert_eq!(r.gain, 0.0);
        assert_eq!(r.tax_owed, 0.0);
    }
}
