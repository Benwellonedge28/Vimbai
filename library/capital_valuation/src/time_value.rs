#![forbid(unsafe_code)]
//! Time value of money, ported from time-value-of-money-service and
//! present-value-service.
//!
//! Both services are pure arithmetic over their request bodies, so the ports
//! take plain numbers and return the same shaped maps the endpoints returned.
//! Rounding follows the Python exactly: `round(x, 2)` for money and
//! `round(x, 6)` for rates and discount factors, with banker's rounding via
//! [`vimbai_common::py_round`].
//!
//! Quirks preserved on purpose:
//!
//! * An unknown `compounding` value falls back to annual, as the Python
//!   `else:` branch does.
//! * Perpetuity with a non-positive rate returns just `{"error": ...}` with no
//!   numeric fields - the original returned that bare dict, not a 400.
//! * Growing perpetuity treats `discount_rate <= growth_rate` the same way.
//! * `pv_with_initial_investment` ignores a residual value of 0 and appends it
//!   as a row keyed by the final year, not a new one.

use serde::{Deserialize, Serialize};
use vimbai_common::py_round;

fn r2(value: f64) -> f64 {
    py_round(value, 2)
}

fn r6(value: f64) -> f64 {
    py_round(value, 6)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FutureValueOutput {
    pub principal: f64,
    pub rate: f64,
    pub years: u32,
    pub compounding: String,
    pub future_value: f64,
    pub interest_earned: f64,
}

/// FV = PV x (1 + r)^n, compounded annually, quarterly, monthly or (fallback) annually.
pub fn future_value(principal: f64, rate: f64, years: u32, compounding: &str) -> FutureValueOutput {
    let factor = match compounding {
        "quarterly" => (1.0 + rate / 4.0).powi((years * 4) as i32),
        "monthly" => (1.0 + rate / 12.0).powi((years * 12) as i32),
        _ => (1.0 + rate).powi(years as i32),
    };
    let future_value = principal * factor;
    FutureValueOutput {
        principal,
        rate,
        years,
        compounding: compounding.to_string(),
        future_value: r2(future_value),
        interest_earned: r2(future_value - principal),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PresentValueOutput {
    pub future_value: f64,
    pub rate: f64,
    pub years: u32,
    pub present_value: f64,
}

/// PV = FV / (1 + r)^n.
pub fn present_value(future_value: f64, rate: f64, years: u32) -> PresentValueOutput {
    let pv = future_value / (1.0 + rate).powi(years as i32);
    PresentValueOutput {
        future_value,
        rate,
        years,
        present_value: r2(pv),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PresentValuePercentOutput {
    pub future_value: f64,
    pub rate_percent: f64,
    pub rate_decimal: f64,
    pub years: u32,
    pub discount_factor: f64,
    pub present_value: f64,
}

/// Same as [`present_value`] but the rate arrives as a percentage.
pub fn present_value_percent(
    future_value: f64,
    rate_percent: f64,
    years: u32,
) -> PresentValuePercentOutput {
    let rate_decimal = rate_percent / 100.0;
    let df = 1.0 / (1.0 + rate_decimal).powi(years as i32);
    PresentValuePercentOutput {
        future_value,
        rate_percent,
        rate_decimal,
        years,
        discount_factor: r6(df),
        present_value: r2(future_value * df),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CompoundInterestOutput {
    pub principal: f64,
    pub rate: f64,
    pub years: u32,
    pub total_amount: f64,
    pub compound_interest: f64,
}

pub fn compound_interest(principal: f64, rate: f64, years: u32) -> CompoundInterestOutput {
    let amount = principal * (1.0 + rate).powi(years as i32);
    CompoundInterestOutput {
        principal,
        rate,
        years,
        total_amount: r2(amount),
        compound_interest: r2(amount - principal),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SimpleInterestOutput {
    pub principal: f64,
    pub rate: f64,
    pub years: u32,
    pub simple_interest: f64,
    pub total_amount: f64,
}

pub fn simple_interest(principal: f64, rate: f64, years: u32) -> SimpleInterestOutput {
    let interest = principal * rate * years as f64;
    SimpleInterestOutput {
        principal,
        rate,
        years,
        simple_interest: r2(interest),
        total_amount: r2(principal + interest),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EffectiveAnnualRateOutput {
    pub nominal_rate: f64,
    pub compounding_per_year: u32,
    pub effective_annual_rate: f64,
    pub effective_rate_percent: String,
}

pub fn effective_annual_rate(
    nominal_rate: f64,
    compounding_per_year: u32,
) -> EffectiveAnnualRateOutput {
    let n = compounding_per_year.max(1) as i32;
    let ear = (1.0 + nominal_rate / n as f64).powi(n) - 1.0;
    EffectiveAnnualRateOutput {
        nominal_rate,
        compounding_per_year: n as u32,
        effective_annual_rate: r6(ear),
        effective_rate_percent: format!("{:.2}%", ear * 100.0),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AnnuityOutput {
    pub payment: f64,
    pub rate: f64,
    pub years: u32,
    pub present_value_annuity: Option<f64>,
    pub future_value_annuity: Option<f64>,
}

/// PV of an ordinary annuity; a zero rate degenerates to `payment * years`.
pub fn pv_annuity(payment: f64, rate: f64, years: u32) -> AnnuityOutput {
    let pv = if rate == 0.0 {
        payment * years as f64
    } else {
        payment * (1.0 - 1.0 / (1.0 + rate).powi(years as i32)) / rate
    };
    AnnuityOutput {
        payment,
        rate,
        years,
        present_value_annuity: Some(r2(pv)),
        future_value_annuity: None,
    }
}

/// FV of an ordinary annuity; a zero rate degenerates to `payment * years`.
pub fn fv_annuity(payment: f64, rate: f64, years: u32) -> AnnuityOutput {
    let fv = if rate == 0.0 {
        payment * years as f64
    } else {
        payment * ((1.0 + rate).powi(years as i32) - 1.0) / rate
    };
    AnnuityOutput {
        payment,
        rate,
        years,
        present_value_annuity: None,
        future_value_annuity: Some(r2(fv)),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LoanRepaymentOutput {
    pub principal: f64,
    pub rate: f64,
    pub years: u32,
    pub annual_repayment: f64,
    pub total_paid: f64,
    pub total_interest: f64,
}

/// Constant annual repayment on an amortising loan (annuity factor inverse).
pub fn loan_repayment(principal: f64, rate: f64, years: u32) -> LoanRepaymentOutput {
    let repayment = if rate == 0.0 {
        principal / years as f64
    } else {
        let f = (1.0 + rate).powi(years as i32);
        principal * rate * f / (f - 1.0)
    };
    let total_paid = repayment * years as f64;
    LoanRepaymentOutput {
        principal,
        rate,
        years,
        annual_repayment: r2(repayment),
        total_paid: r2(total_paid),
        total_interest: r2(total_paid - principal),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CashFlowPresentValue {
    pub year: usize,
    pub cash_flow: f64,
    pub discount_factor: f64,
    pub present_value: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CashFlowsPresentValueOutput {
    pub cash_flows: Vec<f64>,
    pub rate: f64,
    pub present_values: Vec<CashFlowPresentValue>,
    pub total_present_value: f64,
}

/// PV of a series of cash flows, year 1 onwards.
pub fn present_value_of_cash_flows(cash_flows: &[f64], rate: f64) -> CashFlowsPresentValueOutput {
    let mut rows = Vec::with_capacity(cash_flows.len());
    let mut total = 0.0;
    for (index, cash_flow) in cash_flows.iter().enumerate() {
        let year = index + 1;
        let df = 1.0 / (1.0 + rate).powi(year as i32);
        let pv = cash_flow * df;
        total += pv;
        rows.push(CashFlowPresentValue {
            year,
            cash_flow: *cash_flow,
            discount_factor: r6(df),
            present_value: r2(pv),
        });
    }
    CashFlowsPresentValueOutput {
        cash_flows: cash_flows.to_vec(),
        rate,
        present_values: rows,
        total_present_value: r2(total),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PvDetailRow {
    pub year: usize,
    pub cash_flow: Option<f64>,
    pub description: Option<String>,
    pub present_value: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PvWithInvestmentOutput {
    pub initial_investment: f64,
    pub residual_value: f64,
    pub rate: f64,
    pub present_value_details: Vec<PvDetailRow>,
    pub total_pv_inflows: f64,
    pub net_present_value: f64,
    pub decision: String,
}

/// NPV as the present value of inflows minus the initial investment.
pub fn present_value_with_initial_investment(
    initial_investment: f64,
    cash_flows: &[f64],
    rate: f64,
    residual_value: f64,
) -> PvWithInvestmentOutput {
    let mut details = Vec::with_capacity(cash_flows.len() + 1);
    let mut pv_inflows = 0.0;
    for (index, cash_flow) in cash_flows.iter().enumerate() {
        let year = index + 1;
        let pv = cash_flow / (1.0 + rate).powi(year as i32);
        pv_inflows += pv;
        details.push(PvDetailRow {
            year,
            cash_flow: Some(*cash_flow),
            description: None,
            present_value: r2(pv),
        });
    }
    if residual_value > 0.0 {
        let year = cash_flows.len();
        let df = 1.0 / (1.0 + rate).powi(year as i32);
        let pv_residual = residual_value * df;
        pv_inflows += pv_residual;
        details.push(PvDetailRow {
            year,
            cash_flow: None,
            description: Some("Residual value".to_string()),
            present_value: r2(pv_residual),
        });
    }
    let npv = pv_inflows - initial_investment;
    PvWithInvestmentOutput {
        initial_investment,
        residual_value,
        rate,
        present_value_details: details,
        total_pv_inflows: r2(pv_inflows),
        net_present_value: r2(npv),
        decision: if npv > 0.0 {
            "Accept".to_string()
        } else if npv < 0.0 {
            "Reject".to_string()
        } else {
            "Indifferent".to_string()
        },
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PerpetuityOutput {
    pub annual_cash_flow: Option<f64>,
    pub rate: Option<f64>,
    pub present_value_perpetuity: Option<f64>,
    pub formula: Option<String>,
    pub error: Option<String>,
}

/// PV = CF / r. A non-positive rate returns the bare error shape.
pub fn present_value_perpetuity(annual_cash_flow: f64, rate: f64) -> PerpetuityOutput {
    if rate <= 0.0 {
        return PerpetuityOutput {
            annual_cash_flow: None,
            rate: None,
            present_value_perpetuity: None,
            formula: None,
            error: Some("Rate must be positive for perpetuity".to_string()),
        };
    }
    let pv = annual_cash_flow / rate;
    PerpetuityOutput {
        annual_cash_flow: Some(annual_cash_flow),
        rate: Some(rate),
        present_value_perpetuity: Some(r2(pv)),
        formula: Some(format!("{} / {} = {}", annual_cash_flow, rate, pv)),
        error: None,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GrowingPerpetuityOutput {
    pub initial_cash_flow: Option<f64>,
    pub growth_rate: Option<f64>,
    pub discount_rate: Option<f64>,
    pub present_value: Option<f64>,
    pub error: Option<String>,
}

/// PV = CF1 / (r - g) for r > g, else the bare error shape.
pub fn present_value_growing_perpetuity(
    initial_cash_flow: f64,
    growth_rate: f64,
    discount_rate: f64,
) -> GrowingPerpetuityOutput {
    if discount_rate <= growth_rate {
        return GrowingPerpetuityOutput {
            initial_cash_flow: None,
            growth_rate: None,
            discount_rate: None,
            present_value: None,
            error: Some("Discount rate must be greater than growth rate".to_string()),
        };
    }
    GrowingPerpetuityOutput {
        initial_cash_flow: Some(initial_cash_flow),
        growth_rate: Some(growth_rate),
        discount_rate: Some(discount_rate),
        present_value: Some(r2(initial_cash_flow / (discount_rate - growth_rate))),
        error: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn future_value_annual_matches_service() {
        let out = future_value(1000.0, 0.05, 10, "annual");
        assert_eq!(out.future_value, 1628.89);
        assert_eq!(out.interest_earned, 628.89);
    }

    #[test]
    fn future_value_monthly_compounding_is_higher() {
        let annual = future_value(1000.0, 0.05, 10, "annual").future_value;
        let monthly = future_value(1000.0, 0.05, 10, "monthly").future_value;
        assert!(monthly > annual);
        assert_eq!(monthly, 1647.01);
    }

    #[test]
    fn unknown_compounding_falls_back_to_annual() {
        assert_eq!(
            future_value(1000.0, 0.05, 10, "weekly").future_value,
            future_value(1000.0, 0.05, 10, "annual").future_value
        );
    }

    #[test]
    fn present_value_is_the_inverse() {
        assert_eq!(present_value(1628.89, 0.05, 10).present_value, 1000.0);
        let pct = present_value_percent(1000.0, 5.0, 10);
        assert_eq!(pct.rate_decimal, 0.05);
        assert_eq!(pct.discount_factor, 0.613913);
        assert_eq!(pct.present_value, 613.91);
    }

    #[test]
    fn interest_variants_agree_at_one_year() {
        let compound = compound_interest(1000.0, 0.05, 10);
        let simple = simple_interest(1000.0, 0.05, 10);
        assert_eq!(compound.total_amount, 1628.89);
        assert_eq!(compound.compound_interest, 628.89);
        assert_eq!(simple.simple_interest, 500.0);
        assert_eq!(simple.total_amount, 1500.0);
    }

    #[test]
    fn effective_annual_rate_reports_percent_string() {
        let out = effective_annual_rate(0.05, 12);
        assert_eq!(out.effective_annual_rate, 0.051162);
        assert_eq!(out.effective_rate_percent, "5.12%");
    }

    #[test]
    fn annuities_degenerate_at_zero_rate() {
        let pv = pv_annuity(1000.0, 0.0, 5);
        assert_eq!(pv.present_value_annuity, Some(5000.0));
        let fv = fv_annuity(1000.0, 0.0, 5);
        assert_eq!(fv.future_value_annuity, Some(5000.0));
        assert_eq!(
            fv_annuity(1000.0, 0.08, 10).future_value_annuity,
            Some(14486.56)
        );
    }

    #[test]
    fn loan_repayment_amortises() {
        let out = loan_repayment(100000.0, 0.06, 10);
        assert_eq!(out.annual_repayment, 13586.8);
        assert_eq!(out.total_paid, 135867.96);
        assert_eq!(out.total_interest, 35867.96);
        assert_eq!(loan_repayment(100000.0, 0.0, 10).annual_repayment, 10000.0);
    }

    #[test]
    fn cash_flow_schedule_discounts_from_year_one() {
        let out = present_value_of_cash_flows(&[1000.0, 2000.0, 3000.0], 0.10);
        assert_eq!(out.present_values[0].year, 1);
        assert_eq!(out.present_values[0].discount_factor, 0.909091);
        assert_eq!(out.present_values[0].present_value, 909.09);
        assert_eq!(out.total_present_value, 4815.93);
    }

    #[test]
    fn npv_decision_and_residual_row() {
        let out =
            present_value_with_initial_investment(5000.0, &[2000.0, 3000.0, 4000.0], 0.10, 500.0);
        assert_eq!(out.decision, "Accept");
        let last = out.present_value_details.last().unwrap();
        assert_eq!(last.year, 3);
        assert_eq!(last.description.as_deref(), Some("Residual value"));
        // a zero residual appends nothing
        let none =
            present_value_with_initial_investment(5000.0, &[2000.0, 3000.0, 4000.0], 0.10, 0.0);
        assert_eq!(none.present_value_details.len(), 3);
    }

    #[test]
    fn perpetuities_return_bare_error_shape() {
        let flat = present_value_perpetuity(100.0, 0.05);
        assert_eq!(flat.present_value_perpetuity, Some(2000.0));
        let bad = present_value_perpetuity(100.0, 0.0);
        assert!(bad.error.is_some());
        assert!(bad.annual_cash_flow.is_none());

        let growing = present_value_growing_perpetuity(100.0, 0.03, 0.08);
        assert_eq!(growing.present_value, Some(2000.0));
        let inverted = present_value_growing_perpetuity(100.0, 0.09, 0.08);
        assert!(inverted.error.is_some());
    }
}
