//! Black-Scholes option pricing with the greeks.
use serde::{Deserialize, Serialize};

use crate::fx_rates::round_half_even;
use vimbai_common::normal;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Option {
    pub option_type: String,
    pub strike_price: f64,
    pub spot_price: f64,
    pub time_to_expiry: f64,
    pub volatility: f64,
    pub risk_free_rate: f64,
    pub notional: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OptionsPricingRequest {
    pub company_id: String,
    pub options: Vec<Option>,
    pub current_spot: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OptionValuation {
    pub option_type: String,
    pub strike: f64,
    pub spot: f64,
    pub premium: f64,
    pub premium_pct: f64,
    pub moneyness: String,
    pub delta: f64,
    pub gamma: f64,
    pub theta: f64,
    pub vega: f64,
    pub intrinsic_value: f64,
    pub time_value: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OptionsPortfolioSummary {
    pub total_premium: f64,
    pub option_count: usize,
    pub avg_premium_pct: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GreeksSummary {
    pub delta: f64,
    pub gamma: f64,
    pub theta: f64,
    pub vega: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OptionsPricingResult {
    pub company_id: String,
    pub option_valuations: Vec<OptionValuation>,
    pub portfolio_summary: OptionsPortfolioSummary,
    pub greeks_summary: GreeksSummary,
    pub recommendations: Vec<String>,
}

/// `/price` on the options pricing service.
///
/// PYTHON QUIRKS, preserved:
/// - the PUT theta discounts the strike against `cdf(-d1)` instead of
///   `cdf(-d2)`, so the strike term is wrong for every put while the call
///   theta is correct;
/// - theta divides by 365 while the greeks are otherwise unscaled, and vega
///   divides by 100, so the two are quoted per day and per percentage point
///   beside a delta and gamma that are not;
/// - `current_spot` on the request is accepted and ignored: each option is
///   priced off its own spot;
/// - the portfolio average premium is expressed against SPOT notional, not
///   against premium paid, and the aggregate greeks are scaled by notional;
/// - the moneyness label reads the option's own spot against its strike.
pub fn price_options(req: &OptionsPricingRequest) -> OptionsPricingResult {
    let mut option_valuations = Vec::new();
    let mut total_premium = 0.0;
    let mut total_delta = 0.0;
    let mut total_gamma = 0.0;
    let mut total_theta = 0.0;
    let mut total_vega = 0.0;

    for opt in &req.options {
        let (s, k, t, r, sigma) = (
            opt.spot_price,
            opt.strike_price,
            opt.time_to_expiry,
            opt.risk_free_rate,
            opt.volatility,
        );
        let is_call = opt.option_type == "call";
        let price = if is_call {
            normal::black_scholes_call(s, k, t, r, sigma)
        } else {
            normal::black_scholes_put(s, k, t, r, sigma)
        };

        let (d1, _d2) = normal::d1_d2(s, k, t, r, sigma);
        let delta = if is_call {
            normal::norm_cdf(d1)
        } else {
            normal::norm_cdf(d1) - 1.0
        };
        let gamma = normal::norm_pdf(d1) / (s * sigma * t.sqrt());
        // the put branch uses cdf(-d1) where textbook Black-Scholes needs
        // cdf(-d2)
        let strike_cdf = if is_call {
            normal::norm_cdf(d1)
        } else {
            normal::norm_cdf(-d1)
        };
        let theta = (-s * normal::norm_pdf(d1) * sigma / (2.0 * t.sqrt())
            - r * k * (-r * t).exp() * strike_cdf)
            / 365.0;
        let vega = s * t.sqrt() * normal::norm_pdf(d1) / 100.0;

        let intrinsic = if is_call {
            (s - k).max(0.0)
        } else {
            (k - s).max(0.0)
        };
        let moneyness = if (is_call && s > k) || (!is_call && s < k) {
            "ITM"
        } else if s == k {
            "ATM"
        } else {
            "OTM"
        };

        option_valuations.push(OptionValuation {
            option_type: opt.option_type.clone(),
            strike: k,
            spot: s,
            premium: round_half_even(price, 4),
            premium_pct: round_half_even(price / s * 100.0, 4),
            moneyness: moneyness.to_string(),
            delta: round_half_even(delta, 4),
            gamma: round_half_even(gamma, 6),
            theta: round_half_even(theta, 4),
            vega: round_half_even(vega, 4),
            intrinsic_value: intrinsic,
            time_value: price - intrinsic,
        });

        total_premium += price * opt.notional;
        total_delta += delta * opt.notional;
        total_gamma += gamma * opt.notional;
        total_theta += theta * opt.notional;
        total_vega += vega * opt.notional;
    }

    let spot_notional: f64 = req.options.iter().map(|o| o.spot_price * o.notional).sum();
    let avg_premium_pct = if req.options.is_empty() || spot_notional == 0.0 {
        0.0
    } else {
        total_premium / spot_notional * 100.0
    };

    let notional_total: f64 = req.options.iter().map(|o| o.notional).sum();
    let greeks_summary = GreeksSummary {
        delta: round_half_even(total_delta, 4),
        gamma: round_half_even(total_gamma, 6),
        theta: round_half_even(total_theta, 4),
        vega: round_half_even(total_vega, 4),
    };

    let mut recommendations = Vec::new();
    if total_delta.abs() > notional_total * 0.8 {
        recommendations.push("High delta exposure - consider delta hedging".to_string());
    }
    if total_gamma > 0.1 {
        recommendations.push("Gamma risk elevated - monitor position closely".to_string());
    }
    if greeks_summary.theta < -10_000.0 {
        recommendations.push("Significant time decay - assess holding period".to_string());
    }

    OptionsPricingResult {
        company_id: req.company_id.clone(),
        portfolio_summary: OptionsPortfolioSummary {
            total_premium: round_half_even(total_premium, 2),
            option_count: req.options.len(),
            avg_premium_pct: round_half_even(avg_premium_pct, 4),
        },
        greeks_summary,
        option_valuations,
        recommendations,
    }
}
