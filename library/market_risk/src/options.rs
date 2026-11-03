//! European option pricing: Black-Scholes, the greeks, and moneyness.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::money::round_dp;
use crate::normal::{norm_cdf, norm_pdf};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Option {
    /// `call` anything else is priced as a put, as the service did.
    pub option_type: String,
    pub strike_price: f64,
    pub spot_price: f64,
    /// Years to expiry.
    pub time_to_expiry: f64,
    pub volatility: f64,
    pub risk_free_rate: f64,
    pub notional: f64,
}

impl Option {
    pub fn call(
        strike: f64,
        spot: f64,
        time_to_expiry: f64,
        volatility: f64,
        risk_free_rate: f64,
    ) -> Option {
        Option {
            option_type: "call".to_string(),
            strike_price: strike,
            spot_price: spot,
            time_to_expiry,
            volatility,
            risk_free_rate,
            notional: 1.0,
        }
    }

    pub fn put(
        strike: f64,
        spot: f64,
        time_to_expiry: f64,
        volatility: f64,
        risk_free_rate: f64,
    ) -> Option {
        Option {
            option_type: "put".to_string(),
            strike_price: strike,
            spot_price: spot,
            time_to_expiry,
            volatility,
            risk_free_rate,
            notional: 1.0,
        }
    }

    pub fn with_notional(mut self, notional: f64) -> Option {
        self.notional = notional;
        self
    }

    fn is_call(&self) -> bool {
        self.option_type == "call"
    }

    /// Moneyness as the service named it.
    pub fn moneyness(&self) -> &'static str {
        if (self.is_call() && self.spot_price > self.strike_price)
            || (!self.is_call() && self.spot_price < self.strike_price)
        {
            "ITM"
        } else if self.spot_price == self.strike_price {
            "ATM"
        } else {
            "OTM"
        }
    }
}

pub fn black_scholes_call(s: f64, k: f64, t: f64, r: f64, sigma: f64) -> f64 {
    let d1 = ((s / k).ln() + (r + sigma * sigma / 2.0) * t) / (sigma * t.sqrt());
    let d2 = d1 - sigma * t.sqrt();
    s * norm_cdf(d1) - k * (-r * t).exp() * norm_cdf(d2)
}

pub fn black_scholes_put(s: f64, k: f64, t: f64, r: f64, sigma: f64) -> f64 {
    let d1 = ((s / k).ln() + (r + sigma * sigma / 2.0) * t) / (sigma * t.sqrt());
    let d2 = d1 - sigma * t.sqrt();
    k * (-r * t).exp() * norm_cdf(-d2) - s * norm_cdf(-d1)
}

#[derive(Debug, Clone, PartialEq)]
pub struct PricingRequest {
    pub company_id: String,
    pub options: Vec<Option>,
}

impl PricingRequest {
    pub fn new(company_id: &str, options: Vec<Option>) -> PricingRequest {
        PricingRequest {
            company_id: company_id.to_string(),
            options,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PricingResponse {
    pub company_id: String,
    pub option_valuations: Vec<Value>,
    pub portfolio_summary: Value,
    pub greeks_summary: Value,
    pub recommendations: Vec<String>,
}

/// Price a set of European options.
///
/// Preserved: `theta` is divided by 365 whatever the time unit of expiry, so it
/// is a per-day figure for an annual model and understates a per-day decay when
/// expiry is quoted in days; and the delta-hedging recommendation compares a
/// notional-weighted delta against a notional-weighted threshold, so a book with
/// large notionals triggers it on almost any position.
pub fn price(request: &PricingRequest) -> PricingResponse {
    let mut valuations = Vec::new();
    let mut total_premium = 0.0;
    let mut total_delta = 0.0;
    let mut total_gamma = 0.0;
    let mut total_theta = 0.0;
    let mut total_vega = 0.0;
    let mut total_spot_notional = 0.0;

    for option in &request.options {
        let s = option.spot_price;
        let k = option.strike_price;
        let t = option.time_to_expiry;
        let r = option.risk_free_rate;
        let sigma = option.volatility;

        let price = if option.is_call() {
            black_scholes_call(s, k, t, r, sigma)
        } else {
            black_scholes_put(s, k, t, r, sigma)
        };

        let d1 = ((s / k).ln() + (r + sigma * sigma / 2.0) * t) / (sigma * t.sqrt());
        let pdf = norm_pdf(d1);
        let delta = if option.is_call() {
            norm_cdf(d1)
        } else {
            norm_cdf(d1) - 1.0
        };
        let gamma = pdf / (s * sigma * t.sqrt());
        let theta = (-s * pdf * sigma / (2.0 * t.sqrt())
            - r * k * (-r * t).exp() * norm_cdf(if option.is_call() { d1 } else { -d1 }))
            / 365.0;
        let vega = s * t.sqrt() * pdf / 100.0;

        let intrinsic = if option.is_call() {
            (s - k).max(0.0)
        } else {
            (k - s).max(0.0)
        };
        let moneyness = option.moneyness();

        valuations.push(json!({
            "type": option.option_type,
            "strike": k,
            "spot": s,
            "premium": round_dp(price, 4),
            "premium_pct": round_dp(price / s * 100.0, 4),
            "moneyness": moneyness,
            "delta": round_dp(delta, 4),
            "gamma": round_dp(gamma, 6),
            "theta": round_dp(theta, 4),
            "vega": round_dp(vega, 4),
            "intrinsic_value": intrinsic,
            "time_value": price - intrinsic,
        }));

        total_premium += price * option.notional;
        total_delta += delta * option.notional;
        total_gamma += gamma * option.notional;
        total_theta += theta * option.notional;
        total_vega += vega * option.notional;
        total_spot_notional += s * option.notional;
    }

    let count = request.options.len();
    let avg_premium_pct = if count > 0 && total_spot_notional != 0.0 {
        round_dp(total_premium / total_spot_notional * 100.0, 4)
    } else {
        0.0
    };
    let greeks = json!({
        "delta": round_dp(total_delta, 4),
        "gamma": round_dp(total_gamma, 6),
        "theta": round_dp(total_theta, 4),
        "vega": round_dp(total_vega, 4),
    });

    let mut recommendations = Vec::new();
    let total_notional: f64 = request.options.iter().map(|o| o.notional).sum();
    if total_delta.abs() > total_notional * 0.8 {
        recommendations.push("High delta exposure - consider delta hedging".to_string());
    }
    if total_gamma > 0.1 {
        recommendations.push("Gamma risk elevated - monitor position closely".to_string());
    }
    if total_theta < -10_000.0 {
        recommendations.push("Significant time decay - assess holding period".to_string());
    }

    PricingResponse {
        company_id: request.company_id.clone(),
        option_valuations: valuations,
        portfolio_summary: json!({
            "total_premium": round_dp(total_premium, 2),
            "option_count": count,
            "avg_premium_pct": avg_premium_pct,
        }),
        greeks_summary: greeks,
        recommendations,
    }
}
