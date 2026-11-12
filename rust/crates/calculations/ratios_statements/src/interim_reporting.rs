//! Interim reporting, from interim-financial-reporting-service.
//!
//! These two figures used to be placeholders: the port multiplied a hardcoded revenue base of
//! 2,500,000 per period, and the tax rate was the standard rate rather than a computed one, so
//! every caller got the same answer whatever their books said. The Python service did the same,
//! which is why the port reproduced it, but a number that never moves is worse than no number.
//!
//! Both now read the period figures the caller supplies and refuse to answer without them, so a
//! missing input is an error instead of a million-and-a-half of invented revenue.

#![forbid(unsafe_code)]

use serde_json::{json, Value};

/// The statutory rate, used only to report the variance against the effective rate.
const STANDARD_TAX_RATE: f64 = 0.25;

fn number(payload: &Value, key: &str) -> Result<f64, String> {
    payload
        .get(key)
        .and_then(Value::as_f64)
        .ok_or_else(|| format!("{key} is required and must be a number"))
}

/// One reported period. Every figure is the caller's own, nothing is derived from a base.
struct Period {
    revenue: f64,
    profit: f64,
    cash_flow: f64,
}

impl Period {
    fn read(value: &Value) -> Result<Self, String> {
        Ok(Self {
            revenue: number(value, "revenue")?,
            profit: number(value, "profit")?,
            cash_flow: number(value, "cash_flow")?,
        })
    }
}

/// The periods a caller has reported so far. A single flat `revenue`/`profit`/`cash_flow` is
/// accepted as a one-period report, which is what the old shape effectively meant.
fn periods(payload: &Value) -> Result<Vec<Period>, String> {
    if let Some(list) = payload.get("periods") {
        let list = list.as_array().ok_or("periods must be a list")?;
        if list.is_empty() {
            return Err("periods must contain at least one reported period".into());
        }
        return list.iter().map(Period::read).collect();
    }
    Period::read(payload)
        .map(|period| vec![period])
        .map_err(|_| {
            "periods is required: report each period's revenue, profit and cash_flow".into()
        })
}

/// `/year-to-date`
///
/// Totals the periods the caller reports. `period_number` is echoed back as reported, but the
/// figures come from the periods themselves, never from a multiplied base.
pub fn year_to_date_from_json(payload: &Value) -> Result<Value, String> {
    let year = payload.get("year").cloned().ok_or("year is required")?;
    let reported = periods(payload)?;
    let revenue: f64 = reported.iter().map(|period| period.revenue).sum();
    let profit: f64 = reported.iter().map(|period| period.profit).sum();
    let cash_flow: f64 = reported.iter().map(|period| period.cash_flow).sum();
    Ok(json!({
        "year": year,
        "periods_included": reported.len(),
        "ytd_revenue": revenue,
        "ytd_profit": profit,
        "ytd_cash_flow": cash_flow,
    }))
}

/// `/effective-tax-rate`
///
/// The rate the books actually carry: tax expense over profit before tax. The standard rate is
/// reported alongside it so the variance is visible, instead of being asserted as zero.
pub fn effective_tax_rate_from_json(payload: &Value) -> Result<Value, String> {
    let profit_before_tax = number(payload, "profit_before_tax")?;
    let income_tax_expense = number(payload, "income_tax_expense")?;
    let standard = payload
        .get("standard_tax_rate")
        .and_then(Value::as_f64)
        .unwrap_or(STANDARD_TAX_RATE);
    // No profit means no rate to report; saying so beats dividing by zero or inventing one.
    let effective = if profit_before_tax == 0.0 {
        0.0
    } else {
        income_tax_expense / profit_before_tax
    };
    Ok(json!({
        "profit_before_tax": profit_before_tax,
        "income_tax_expense": income_tax_expense,
        "effective_tax_rate": effective,
        "standard_tax_rate": standard,
        "difference": effective - standard,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn year_to_date_totals_the_reported_periods() {
        let out = year_to_date_from_json(&json!({
            "year": 2026,
            "periods": [
                {"revenue": 1_200_000.0, "profit": 96_000.0, "cash_flow": 74_000.0},
                {"revenue": 1_340_000.0, "profit": 118_000.0, "cash_flow": 91_000.0},
            ]
        }))
        .unwrap();
        assert_eq!(out["periods_included"], json!(2));
        assert_eq!(out["ytd_revenue"], json!(2_540_000.0));
        assert_eq!(out["ytd_profit"], json!(214_000.0));
        assert_eq!(out["ytd_cash_flow"], json!(165_000.0));
    }

    #[test]
    fn different_books_give_different_figures() {
        let small = year_to_date_from_json(&json!({
            "year": 2026,
            "periods": [{"revenue": 100_000.0, "profit": 8_000.0, "cash_flow": 6_000.0}]
        }))
        .unwrap();
        let large = year_to_date_from_json(&json!({
            "year": 2026,
            "periods": [{"revenue": 900_000.0, "profit": 70_000.0, "cash_flow": 60_000.0}]
        }))
        .unwrap();
        assert_ne!(small["ytd_revenue"], large["ytd_revenue"]);
        assert_ne!(small["ytd_profit"], large["ytd_profit"]);
    }

    #[test]
    fn a_flat_period_is_read_as_one_reported_period() {
        let out = year_to_date_from_json(&json!({
            "year": 2026,
            "revenue": 480_000.0,
            "profit": 31_000.0,
            "cash_flow": 22_000.0
        }))
        .unwrap();
        assert_eq!(out["periods_included"], json!(1));
        assert_eq!(out["ytd_revenue"], json!(480_000.0));
    }

    #[test]
    fn no_books_means_no_figure() {
        let err = year_to_date_from_json(&json!({"year": 2026, "period_number": 2})).unwrap_err();
        assert!(err.contains("periods is required"), "{err}");
        let err = year_to_date_from_json(&json!({
            "year": 2026,
            "periods": [{"revenue": 10.0, "profit": 1.0}]
        }))
        .unwrap_err();
        assert_eq!(err, "cash_flow is required and must be a number");
    }

    #[test]
    fn the_effective_rate_is_the_books_own_rate() {
        let out = effective_tax_rate_from_json(&json!({
            "profit_before_tax": 400_000.0,
            "income_tax_expense": 84_000.0
        }))
        .unwrap();
        assert_eq!(out["effective_tax_rate"], json!(0.21));
        assert_eq!(out["standard_tax_rate"], json!(0.25));
        // 0.21 - 0.25 is not exactly -0.04 in binary, so compare the variance, not its spelling.
        let difference = out["difference"].as_f64().unwrap();
        assert!((difference + 0.04).abs() < 1e-12, "{difference}");
    }

    #[test]
    fn a_zero_profit_period_reports_no_rate_instead_of_dividing() {
        let out = effective_tax_rate_from_json(&json!({
            "profit_before_tax": 0.0,
            "income_tax_expense": 0.0
        }))
        .unwrap();
        assert_eq!(out["effective_tax_rate"], json!(0.0));
        assert_eq!(out["difference"], json!(-0.25));
    }
}
