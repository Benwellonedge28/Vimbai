//! Interim reporting, from interim-financial-reporting-service.
//!
//! Read the two figures before trusting them. The service computes both from a hardcoded
//! revenue base of 2,500,000 per period rather than from any period the caller supplies,
//! so these are placeholders that always answer the same shape. They are ported as they
//! behave, because a caller comparing the two front ends must get the same number, and the
//! base is named here so nobody mistakes it for a calculation over real books.

#![forbid(unsafe_code)]

use serde_json::{json, Value};

const REVENUE_PER_PERIOD: f64 = 2_500_000.0;
const PROFIT_MARGIN: f64 = 0.12;
const STANDARD_TAX_RATE: f64 = 0.25;

fn field(payload: &Value, key: &str) -> Result<Value, String> {
    payload
        .get(key)
        .cloned()
        .ok_or_else(|| format!("{key} is required"))
}

fn period_number(payload: &Value) -> Result<f64, String> {
    let periods = payload
        .get("period_number")
        .and_then(Value::as_f64)
        .ok_or("period_number is required")?;
    Ok(periods)
}

/// `/year-to-date`
pub fn year_to_date_from_json(payload: &Value) -> Result<Value, String> {
    let year = field(payload, "year")?;
    let periods = period_number(payload)?;
    let revenue = REVENUE_PER_PERIOD * periods;
    let profit = revenue * PROFIT_MARGIN;
    Ok(json!({
        "year": year,
        "periods_included": periods,
        "ytd_revenue": revenue,
        "ytd_profit": profit,
        "ytd_cash_flow": profit * 0.8,
    }))
}

/// `/effective-tax-rate`
pub fn effective_tax_rate_from_json(payload: &Value) -> Result<Value, String> {
    let periods = period_number(payload)?;
    let profit_before_tax = REVENUE_PER_PERIOD * periods * PROFIT_MARGIN;
    let income_tax_expense = profit_before_tax * STANDARD_TAX_RATE;
    Ok(json!({
        "profit_before_tax": profit_before_tax,
        "income_tax_expense": income_tax_expense,
        "effective_tax_rate": STANDARD_TAX_RATE,
        "standard_tax_rate": STANDARD_TAX_RATE,
        "difference": 0.0,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn year_to_date_multiplies_the_placeholder_base() {
        let out = year_to_date_from_json(&json!({
            "period_type": "quarterly",
            "year": 2026,
            "period_number": 2,
            "start_date": "2026-01-01",
            "end_date": "2026-06-30"
        }))
        .unwrap();
        assert_eq!(out["ytd_revenue"], json!(5_000_000.0));
        assert_eq!(out["ytd_profit"], json!(600_000.0));
        assert_eq!(out["ytd_cash_flow"], json!(480_000.0));
    }

    #[test]
    fn the_effective_rate_matches_the_standard_rate_exactly() {
        let out = effective_tax_rate_from_json(&json!({"period_number": 3})).unwrap();
        assert_eq!(out["effective_tax_rate"], json!(0.25));
        assert_eq!(out["difference"], json!(0.0));
        assert_eq!(out["profit_before_tax"], json!(900_000.0));
    }
}
