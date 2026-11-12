#![forbid(unsafe_code)]
//! Payback period, ported from payback-period-service.
//!
//! Two shapes preserved exactly:
//!
//! 1. All cash flows equal: payback = investment / flow, and a
//!    non-positive flow is an error ("Annual cash flow must be positive").
//! 2. Unequal flows: cumulative walk; payback achieved in the first year
//!    the cumulative inflow covers the investment, interpolated as
//!    `(i - 1) + remaining / cf` (0 partial when cf == 0); not recovered
//!    within the horizon => `payback_period: None`.

use serde::{Deserialize, Serialize};
use vimbai_common::py_round2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaybackOutput {
    pub initial_investment: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annual_cash_flows: Option<Vec<f64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cash_flow_schedule: Option<Vec<CashFlowYear>>,
    pub equal_cash_flows: bool,
    pub payback_period: Option<f64>,
    pub payback_years: Option<String>,
    pub payback_achieved_in_year: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CashFlowYear {
    pub year: usize,
    pub cash_flow: f64,
    pub cumulative: f64,
}

fn human_years(payback: f64) -> String {
    format!(
        "{} years and {:.1} months",
        payback.trunc() as i64,
        (payback % 1.0) * 12.0
    )
}

/// Payback period (payback-period-service).
pub fn payback_period(initial_investment: f64, annual_cash_flows: &[f64]) -> PaybackOutput {
    // Equal cash flows
    if !annual_cash_flows.is_empty() && annual_cash_flows.iter().all(|f| *f == annual_cash_flows[0])
    {
        let annual_cf = annual_cash_flows[0];
        if annual_cf <= 0.0 {
            return PaybackOutput {
                initial_investment,
                annual_cash_flows: Some(annual_cash_flows.to_vec()),
                cash_flow_schedule: None,
                equal_cash_flows: true,
                payback_period: None,
                payback_years: None,
                payback_achieved_in_year: None,
                error: Some(
                    "Annual cash flow must be positive for payback calculation".to_string(),
                ),
            };
        }
        let payback = initial_investment / annual_cf;
        return PaybackOutput {
            initial_investment,
            annual_cash_flows: Some(annual_cash_flows.to_vec()),
            cash_flow_schedule: None,
            equal_cash_flows: true,
            payback_period: Some(py_round2(payback)),
            payback_years: Some(human_years(payback)),
            payback_achieved_in_year: None,
            error: None,
        };
    }

    // Unequal cash flows: cumulative walk
    let mut cumulative = 0.0;
    let mut schedule = Vec::with_capacity(annual_cash_flows.len());
    for (i, cf) in annual_cash_flows.iter().enumerate() {
        cumulative += cf;
        schedule.push(CashFlowYear {
            year: i + 1,
            cash_flow: *cf,
            cumulative: py_round2(cumulative),
        });

        if cumulative >= initial_investment {
            let remaining = initial_investment - (cumulative - cf);
            let partial_year = if *cf != 0.0 { remaining / cf } else { 0.0 };
            let payback = i as f64 + partial_year;
            return PaybackOutput {
                initial_investment,
                annual_cash_flows: None,
                cash_flow_schedule: Some(schedule),
                equal_cash_flows: false,
                payback_period: Some(py_round2(payback)),
                payback_years: Some(human_years(payback)),
                payback_achieved_in_year: Some(i + 1),
                error: None,
            };
        }
    }

    PaybackOutput {
        initial_investment,
        annual_cash_flows: None,
        cash_flow_schedule: Some(schedule),
        equal_cash_flows: false,
        payback_period: None,
        payback_years: None,
        payback_achieved_in_year: None,
        error: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equal_flows_simple_division() {
        // python: inv=10000, flows=[2500 x5] -> 4.0 "4 years and 0.0 months"
        let out = payback_period(10000.0, &[2500.0, 2500.0, 2500.0, 2500.0, 2500.0]);
        assert_eq!(out.payback_period, Some(4.0));
        assert_eq!(out.payback_years.as_deref(), Some("4 years and 0.0 months"));
        assert!(out.equal_cash_flows);
    }

    #[test]
    fn equal_nonpositive_flow_is_error() {
        let out = payback_period(1000.0, &[0.0, 0.0]);
        assert!(out.error.is_some());
        assert_eq!(out.payback_period, None);
    }

    #[test]
    fn varying_flows_partial_year() {
        // inv=10000, flows=[4000,3000,2000,1000]: cum=9000 at y3, remaining 1000/1000
        // payback = 3 + 1.0 = 4.0, achieved in year 4
        let out = payback_period(10000.0, &[4000.0, 3000.0, 2000.0, 1000.0]);
        assert_eq!(out.payback_period, Some(4.0));
        assert_eq!(out.payback_years.as_deref(), Some("4 years and 0.0 months"));
        assert_eq!(out.payback_achieved_in_year, Some(4));
        assert_eq!(out.cash_flow_schedule.as_ref().unwrap().len(), 4);
    }

    #[test]
    fn never_recovers() {
        let out = payback_period(10000.0, &[1000.0, 2000.0]);
        assert_eq!(out.payback_period, None);
        assert!(out.error.is_none());
    }
}
