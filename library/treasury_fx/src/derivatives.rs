//! Derivative and hedge analytics: cross-currency swaps, interest rate
//! swap valuation, futures positions, commodity hedges and interest rate
//! sensitivity.
use serde::{Deserialize, Serialize};

use crate::fx_rates::round_half_even;

// ---------------------------------------------------------------- swaps

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CrossCurrencySwap {
    pub swap_id: String,
    pub notional_1: f64,
    pub currency_1: String,
    pub rate_1: f64,
    pub notional_2: f64,
    pub currency_2: String,
    pub rate_2: f64,
    pub tenor_years: f64,
    pub exchange_at_inception: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CrossCurrencySwapRequest {
    pub company_id: String,
    pub swaps: Vec<CrossCurrencySwap>,
    pub fx_spot: Vec<(String, f64)>,
    pub ois_rates: Vec<(String, f64)>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CrossCurrencySwapValuation {
    pub swap_id: String,
    pub notional_1: f64,
    pub currency_1: String,
    pub notional_2: f64,
    pub currency_2: String,
    pub rate_1: f64,
    pub rate_2: f64,
    pub tenor_years: f64,
    pub pv_leg_1: f64,
    pub pv_leg_2: f64,
    pub mtm: f64,
    pub mtm_currency1: f64,
    pub cross_currency_basis: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CurrencyExposure {
    pub total_mtm: f64,
    pub by_currency: Vec<(String, f64)>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CrossCurrencySwapResult {
    pub company_id: String,
    pub swap_valuations: Vec<CrossCurrencySwapValuation>,
    pub currency_exposure: CurrencyExposure,
    pub recommendations: Vec<String>,
}

fn lookup(table: &[(String, f64)], key: &str) -> Option<f64> {
    table.iter().find(|(k, _)| k == key).map(|(_, v)| *v)
}

fn add_exposure(map: &mut Vec<(String, f64)>, key: &str, delta: f64) {
    if let Some(slot) = map.iter_mut().find(|(k, _)| k == key) {
        slot.1 += delta;
    } else {
        map.push((key.to_string(), delta));
    }
}

/// `/value` on the cross-currency swap service.
///
/// PYTHON QUIRKS, all preserved:
/// - discounting uses the OIS rate for a currency when one is supplied but
///   the COUPON uses the swap's contractual rate, so a supplied OIS rate
///   changes the discount factors without changing the coupon;
/// - for tenors up to five years the coupon leg is the rate times the SUM
///   OF ANNUAL DISCOUNT FACTORS, with no payment frequency and no year
///   fraction, while a fractional tenor simply drops the years it does not
///   reach;
/// - beyond five years the coupon leg divides by the discount rate, giving
///   notional times `rate / OIS * (1 - exp(-OIS t))`. When no OIS rate is
///   supplied the two rates ARE the same and cancel, collapsing the leg to
///   the notional less its discount; when one is supplied they differ and
///   the contractual rate is amplified instead;
/// - the second leg is translated at the currency pair's spot rate, which
///   defaults to 1 when absent;
/// - the reported `pv_leg` figures include the coupon legs;
/// - the cross-currency basis is the contractual rate difference in basis
///   points, not a market basis.
pub fn value_cross_currency_swaps(req: &CrossCurrencySwapRequest) -> CrossCurrencySwapResult {
    let mut swap_valuations = Vec::new();
    let mut total_mtm = 0.0;
    let mut exposure_by_currency: Vec<(String, f64)> = Vec::new();

    for swap in &req.swaps {
        let rate_1 = lookup(&req.ois_rates, &swap.currency_1).unwrap_or(swap.rate_1);
        let rate_2 = lookup(&req.ois_rates, &swap.currency_2).unwrap_or(swap.rate_2);
        let fx = lookup(
            &req.fx_spot,
            &format!("{}/{}", swap.currency_1, swap.currency_2),
        )
        .unwrap_or(1.0);

        let pv_leg_1 = swap.notional_1 * (-rate_1 * swap.tenor_years).exp();
        let pv_leg_2 = swap.notional_2 * (-rate_2 * swap.tenor_years).exp();

        let pv_interest_1 = if swap.tenor_years <= 5.0 {
            let sum: f64 = [1.0, 2.0, 3.0, 4.0, 5.0]
                .iter()
                .filter(|t| **t <= swap.tenor_years)
                .map(|t| (-rate_1 * t).exp())
                .sum();
            swap.notional_1 * swap.rate_1 * sum
        } else {
            // the coupon rate divides out, leaving the par approximation
            swap.notional_1 * swap.rate_1 / rate_1 * (1.0 - (-rate_1 * swap.tenor_years).exp())
        };
        let pv_interest_2 = if swap.tenor_years <= 5.0 {
            let sum: f64 = [1.0, 2.0, 3.0, 4.0, 5.0]
                .iter()
                .filter(|t| **t <= swap.tenor_years)
                .map(|t| (-rate_2 * t).exp())
                .sum();
            swap.notional_2 * swap.rate_2 * sum
        } else {
            swap.notional_2 * swap.rate_2 / rate_2 * (1.0 - (-rate_2 * swap.tenor_years).exp())
        };

        let mtm_1 = pv_leg_1 + pv_interest_1;
        let mtm_2 = (pv_leg_2 + pv_interest_2) * fx;
        let net_mtm = mtm_1 - mtm_2;

        swap_valuations.push(CrossCurrencySwapValuation {
            swap_id: swap.swap_id.clone(),
            notional_1: swap.notional_1,
            currency_1: swap.currency_1.clone(),
            notional_2: swap.notional_2,
            currency_2: swap.currency_2.clone(),
            rate_1: swap.rate_1,
            rate_2: swap.rate_2,
            tenor_years: swap.tenor_years,
            pv_leg_1: round_half_even(mtm_1, 2),
            pv_leg_2: round_half_even(mtm_2, 2),
            mtm: round_half_even(net_mtm, 2),
            mtm_currency1: round_half_even(net_mtm, 2),
            cross_currency_basis: round_half_even((rate_2 - rate_1) * 100.0, 4),
        });

        total_mtm += net_mtm;
        add_exposure(&mut exposure_by_currency, &swap.currency_1, swap.notional_1);
        add_exposure(
            &mut exposure_by_currency,
            &swap.currency_2,
            -swap.notional_2,
        );
    }

    let mut recommendations = Vec::new();
    if total_mtm.abs() > 1_000_000.0 {
        recommendations.push("Significant MTM - review counterparty exposure".to_string());
    }
    if exposure_by_currency.len() > 3 {
        recommendations
            .push("Complex multi-currency exposure - simplify where possible".to_string());
    }

    CrossCurrencySwapResult {
        company_id: req.company_id.clone(),
        currency_exposure: CurrencyExposure {
            total_mtm: round_half_even(total_mtm, 2),
            by_currency: exposure_by_currency
                .into_iter()
                .map(|(k, v)| (k, round_half_even(v, 2)))
                .collect(),
        },
        swap_valuations,
        recommendations,
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Swap {
    pub swap_id: String,
    pub swap_type: String,
    pub notional: f64,
    pub fixed_rate: f64,
    pub floating_rate: f64,
    pub tenor_years: u32,
    pub payment_frequency: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SwapValuationRequest {
    pub company_id: String,
    pub swaps: Vec<Swap>,
    pub current_yield_curve: Vec<(String, f64)>,
    pub ois_rate: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SwapValuation {
    pub swap_id: String,
    pub swap_type: String,
    pub notional: f64,
    pub fixed_rate: f64,
    pub floating_rate: f64,
    pub fixed_leg_pv: f64,
    pub floating_leg_pv: f64,
    pub market_value: f64,
    pub market_value_pct: f64,
    pub receive_fixed: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PortfolioSummary {
    pub total_mtm: f64,
    pub receive_fixed_pv: f64,
    pub pay_fixed_pv: f64,
    pub swap_count: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CounterpartyExposure {
    pub net_exposure: f64,
    pub gross_exposure: f64,
    pub largest_exposure: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SwapValuationResult {
    pub company_id: String,
    pub swap_valuations: Vec<SwapValuation>,
    pub portfolio_summary: PortfolioSummary,
    pub counterparty_exposure: CounterpartyExposure,
    pub recommendations: Vec<String>,
}

/// `/value` on the swap valuation service.
///
/// PYTHON QUIRKS, preserved:
/// - only "semi-annual" and "quarterly" set the payment frequency; every
///   other label, including monthly and annual, is treated as annual;
/// - the discount amount is a SUM of discount factors at every payment
///   date, never averaged or scaled by the period, so longer tenors and
///   higher frequencies inflate both legs;
/// - both legs use the same discount amount, so the market value is simply
///   the rate difference times the notional, and neither the swap type nor
///   the yield curve has any effect;
/// - "receive_fixed" is inferred from the sign rather than the contract;
/// - the counterparty figures are built from the ROUNDED market values.
pub fn value_swaps(req: &SwapValuationRequest) -> SwapValuationResult {
    let mut swap_valuations = Vec::new();
    let mut total_mtm = 0.0;

    for swap in &req.swaps {
        let payments_per_year = if swap.payment_frequency == "semi-annual" {
            2u32
        } else if swap.payment_frequency == "quarterly" {
            4u32
        } else {
            1u32
        };

        let periods = swap.tenor_years * payments_per_year;
        let discount_factor: f64 = (1..=periods)
            .map(|i| 1.0 / (1.0 + req.ois_rate).powf(i as f64 / payments_per_year as f64))
            .sum();

        let fixed_pv = swap.notional * swap.fixed_rate / payments_per_year as f64 * discount_factor;
        let floating_pv =
            swap.notional * swap.floating_rate / payments_per_year as f64 * discount_factor;
        let mtm = fixed_pv - floating_pv;

        swap_valuations.push(SwapValuation {
            swap_id: swap.swap_id.clone(),
            swap_type: swap.swap_type.clone(),
            notional: swap.notional,
            fixed_rate: swap.fixed_rate,
            floating_rate: swap.floating_rate,
            fixed_leg_pv: round_half_even(fixed_pv, 2),
            floating_leg_pv: round_half_even(floating_pv, 2),
            market_value: round_half_even(mtm, 2),
            market_value_pct: round_half_even(
                if swap.notional != 0.0 {
                    mtm / swap.notional * 100.0
                } else {
                    0.0
                },
                4,
            ),
            receive_fixed: mtm > 0.0,
        });
        total_mtm += mtm;
    }

    let receive_fixed: f64 = swap_valuations
        .iter()
        .filter(|s| s.receive_fixed)
        .map(|s| s.market_value)
        .sum();
    let pay_fixed: f64 = swap_valuations
        .iter()
        .filter(|s| !s.receive_fixed)
        .map(|s| s.market_value)
        .sum();

    let counterparty_exposure = CounterpartyExposure {
        net_exposure: round_half_even((receive_fixed + pay_fixed).max(0.0), 2),
        gross_exposure: round_half_even(receive_fixed + pay_fixed.abs(), 2),
        // the largest single market value, which can itself be negative
        largest_exposure: if swap_valuations.is_empty() {
            0.0
        } else {
            swap_valuations
                .iter()
                .map(|s| s.market_value)
                .fold(f64::NEG_INFINITY, f64::max)
        },
    };

    let total_notional: f64 = req.swaps.iter().map(|s| s.notional).sum();
    let mut recommendations = Vec::new();
    if total_mtm < 0.0 && total_mtm.abs() > total_notional * 0.1 {
        recommendations.push("Significant negative MTM - monitor counterparty risk".to_string());
    }
    if counterparty_exposure.largest_exposure > total_notional * 0.3 {
        recommendations.push(
            "Concentrated exposure to single counterparty - consider diversification".to_string(),
        );
    }

    SwapValuationResult {
        company_id: req.company_id.clone(),
        portfolio_summary: PortfolioSummary {
            total_mtm: round_half_even(total_mtm, 2),
            receive_fixed_pv: round_half_even(receive_fixed, 2),
            pay_fixed_pv: round_half_even(pay_fixed, 2),
            swap_count: req.swaps.len(),
        },
        counterparty_exposure,
        swap_valuations,
        recommendations,
    }
}
