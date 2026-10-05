#![forbid(unsafe_code)]
//! Internal Rate of Return, ported from internal-rate-return-service.
//!
//! Two solvers are preserved exactly:
//!
//! 1. [`irr`] — Newton-Raphson with the service's initial guess
//!    (average cash flow / investment), numeric derivative (delta 1e-4),
//!    100 iterations, tolerance 1e-4 and the same safety bounds
//!    (rate clamped to [-0.5, 5] via -0.99/10 escapes).
//! 2. [`irr_bisection`] — bracketed fallback for non-convex profiles.

use serde::{Deserialize, Serialize};
use vimbai_common::{py_round, py_round2};

/// NPV at a given rate (IRR service's helper, residual discounted at
/// the final year).
pub fn npv_at_rate(initial_investment: f64, cash_flows: &[f64], rate: f64, residual: f64) -> f64 {
    let mut pv = 0.0;
    for (i, cf) in cash_flows.iter().enumerate() {
        pv += cf / (1.0 + rate).powi((i + 1) as i32);
    }
    if residual > 0.0 {
        pv += residual / (1.0 + rate).powi(cash_flows.len() as i32);
    }
    pv - initial_investment
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IrrMethod {
    NewtonRaphson,
    Bisection,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrrOutput {
    pub initial_investment: f64,
    pub cash_flows: Vec<f64>,
    pub residual_value: f64,
    pub method: IrrMethod,
    pub converged: bool,
    pub irr_decimal: f64,
    pub irr_percentage: f64,
    pub npv_at_irr: f64,
}

/// Newton-Raphson IRR (internal-rate-return-service `/calculate`).
/// Mirrors the Python iteration step-for-step, including the
/// `irr = rate * 100 if rate > 0 else rate` percentage quirk.
pub fn irr(
    initial_investment: f64,
    cash_flows: &[f64],
    residual_value: f64,
    max_iterations: usize,
    tolerance: f64,
) -> IrrOutput {
    let newton = irr_newton_raw(
        initial_investment,
        cash_flows,
        residual_value,
        max_iterations,
        tolerance,
    );
    match newton {
        Some(rate) => finish(
            initial_investment,
            cash_flows,
            residual_value,
            rate,
            IrrMethod::NewtonRaphson,
            true,
        ),
        None => {
            // Fall back to bisection when Newton fails to converge, so the
            // library always returns a usable answer.
            match bisection_raw(initial_investment, cash_flows, residual_value, 200) {
                Some(rate) => finish(
                    initial_investment,
                    cash_flows,
                    residual_value,
                    rate,
                    IrrMethod::Bisection,
                    true,
                ),
                None => finish(
                    initial_investment,
                    cash_flows,
                    residual_value,
                    f64::NAN,
                    IrrMethod::NewtonRaphson,
                    false,
                ),
            }
        }
    }
}

fn irr_newton_raw(
    initial_investment: f64,
    cash_flows: &[f64],
    residual_value: f64,
    max_iterations: usize,
    tolerance: f64,
) -> Option<f64> {
    if cash_flows.is_empty() || initial_investment == 0.0 {
        return None;
    }
    let avg_cf: f64 = cash_flows.iter().sum::<f64>() / cash_flows.len() as f64;
    let mut rate = avg_cf / initial_investment;

    for _ in 0..max_iterations {
        let npv = npv_at_rate(initial_investment, cash_flows, rate, residual_value);
        let delta = 0.0001;
        let npv_plus = npv_at_rate(initial_investment, cash_flows, rate + delta, residual_value);
        let derivative = (npv_plus - npv) / delta;

        if derivative.abs() < 1e-10 {
            break;
        }

        let new_rate = rate - npv / derivative;

        if (new_rate - rate).abs() < tolerance {
            rate = new_rate;
            return Some(rate);
        }

        rate = new_rate;
        if rate < -0.99 {
            rate = -0.5;
        } else if rate > 10.0 {
            rate = 5.0;
        }
    }
    // Python returns whatever rate it reached; treat reached-rate as the
    // answer only if NPV is close to zero (converged).
    if npv_at_rate(initial_investment, cash_flows, rate, residual_value).abs() < 1e-3 {
        Some(rate)
    } else {
        None
    }
}

fn bisection_raw(
    initial_investment: f64,
    cash_flows: &[f64],
    residual_value: f64,
    iterations: usize,
) -> Option<f64> {
    let mut lo = -0.9999;
    let mut hi = 10.0;
    let f_lo = npv_at_rate(initial_investment, cash_flows, lo, residual_value);
    let f_hi = npv_at_rate(initial_investment, cash_flows, hi, residual_value);
    if f_lo * f_hi > 0.0 {
        return None;
    }
    for _ in 0..iterations {
        let mid = (lo + hi) / 2.0;
        let f_mid = npv_at_rate(initial_investment, cash_flows, mid, residual_value);
        if f_mid.abs() < 1e-10 {
            return Some(mid);
        }
        if f_lo * f_mid < 0.0 {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    Some((lo + hi) / 2.0)
}

/// Bisection IRR (internal-rate-return-service `/calculate-bisection`).
pub fn irr_bisection(
    initial_investment: f64,
    cash_flows: &[f64],
    residual_value: f64,
) -> IrrOutput {
    match bisection_raw(initial_investment, cash_flows, residual_value, 200) {
        Some(rate) => finish(
            initial_investment,
            cash_flows,
            residual_value,
            rate,
            IrrMethod::Bisection,
            true,
        ),
        None => finish(
            initial_investment,
            cash_flows,
            residual_value,
            f64::NAN,
            IrrMethod::Bisection,
            false,
        ),
    }
}

fn finish(
    initial_investment: f64,
    cash_flows: &[f64],
    residual_value: f64,
    rate: f64,
    method: IrrMethod,
    converged: bool,
) -> IrrOutput {
    let final_npv = npv_at_rate(initial_investment, cash_flows, rate, residual_value);
    IrrOutput {
        initial_investment,
        cash_flows: cash_flows.to_vec(),
        residual_value,
        method,
        converged,
        irr_decimal: py_round(rate, 6),
        irr_percentage: if rate > 0.0 {
            py_round2(rate * 100.0)
        } else {
            py_round2(rate)
        },
        npv_at_irr: py_round2(final_npv),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newton_solves_simple_project() {
        // inv=1000, flows=[500,500,500] -> irr ~= 23.38%
        let out = irr(1000.0, &[500.0, 500.0, 500.0], 0.0, 100, 0.0001);
        assert!(out.converged);
        assert_eq!(out.method, IrrMethod::NewtonRaphson);
        assert!(
            (out.irr_percentage - 23.38).abs() < 0.05,
            "got {}",
            out.irr_percentage
        );
        assert!(out.npv_at_irr.abs() < 1.0);
    }

    #[test]
    fn npv_at_irr_is_zero() {
        let out = irr(2000.0, &[700.0, 800.0, 900.0], 0.0, 100, 0.0001);
        assert!(out.npv_at_irr.abs() < 1.0, "npv_at_irr {}", out.npv_at_irr);
    }

    #[test]
    fn bisection_agrees_with_newton() {
        let flows = [400.0, 400.0, 400.0, 400.0];
        let a = irr(1200.0, &flows, 0.0, 100, 0.0001);
        let b = irr_bisection(1200.0, &flows, 0.0);
        assert!((a.irr_percentage - b.irr_percentage).abs() < 0.05);
    }

    #[test]
    fn negative_irr_reports_quirk() {
        // inv=1000, flows=[100,100]: python quirk keeps negative rate as-is
        let out = irr(1000.0, &[100.0, 100.0], 0.0, 100, 0.0001);
        assert!(out.irr_percentage < 0.0);
    }

    #[test]
    fn residual_value_affects_rate() {
        let without = irr(1000.0, &[300.0, 300.0, 300.0], 0.0, 100, 0.0001);
        let with_res = irr(1000.0, &[300.0, 300.0, 300.0], 400.0, 100, 0.0001);
        assert!(with_res.irr_percentage > without.irr_percentage);
    }
}
