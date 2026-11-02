//! exotic-derivatives-service: vanilla Black-Scholes with four exotics priced
//! by adjustment to it.
//!
//! The adjustments are the service's own approximations, reproduced exactly:
//! an Asian uses a scaled volatility and half the tenor, a lookback simply
//! multiplies the vanilla price by 1.5, a binary prices the cash-or-nothing
//! digital off d2, and a barrier scales the vanilla by a normal probability of
//! touching the level. Gamma, vega, theta and rho are always the vanilla
//! greeks even for the exotics, because the service computes them before the
//! branch.

use serde::{Deserialize, Serialize};
use vimbai_common::py_round;

fn norm_cdf(value: f64) -> f64 {
    0.5 * (1.0 + erf(value / std::f64::consts::SQRT_2))
}

fn norm_pdf(value: f64) -> f64 {
    (-0.5 * value * value).exp() / (2.0 * std::f64::consts::PI).sqrt()
}

/// `math.erf` to about 1e-14: a Maclaurin series for the middle of the range
/// and the asymptotic complementary expansion beyond |x| = 3. The services
/// quote prices to four places, but gamma is rounded to six, so a 1e-7
/// polynomial approximation is not good enough here.
fn erf(x: f64) -> f64 {
    let sign = if x < 0.0 { -1.0 } else { 1.0 };
    let x = x.abs();
    let value = if x < 3.0 {
        let mut term = x;
        let mut sum = x;
        let mut n = 1u32;
        loop {
            term *= -x * x / n as f64;
            let add = term / (2 * n + 1) as f64;
            sum += add;
            n += 1;
            if add.abs() < sum.abs() * 1e-18 || n > 200 {
                break;
            }
        }
        sum * 2.0 / std::f64::consts::PI.sqrt()
    } else if x < 40.0 {
        // erfc(x) = e^(-x^2)/(x sqrt(pi)) * sum (-1)^n (2n-1)!!/(2x^2)^n
        1.0 - erfc_asymptotic(x)
    } else {
        1.0
    };
    sign * value
}

fn erfc_asymptotic(x: f64) -> f64 {
    let mut term = 1.0;
    let mut sum = 1.0;
    let mut n = 1.0;
    loop {
        let next = term * (2.0 * n - 1.0) / (2.0 * x * x);
        if next.abs() > term.abs() {
            break;
        }
        term = -next;
        sum += term;
        n += 1.0;
        if term.abs() < 1e-18 {
            break;
        }
    }
    sum * (-x * x).exp() / (x * std::f64::consts::PI.sqrt())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DerivativeRequest {
    pub option_type: String,
    #[serde(default)]
    pub underlying: String,
    pub spot_price: f64,
    pub strike_price: f64,
    pub volatility: f64,
    pub risk_free_rate: f64,
    pub time_to_expiry: f64,
    #[serde(default)]
    pub barrier_price: f64,
    #[serde(default)]
    pub barrier_type: String,
    #[serde(default)]
    pub is_call: bool,
    #[serde(default)]
    pub observations: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DerivativeResult {
    pub option_type: String,
    pub underlying: String,
    pub estimated_price: f64,
    pub delta: f64,
    pub gamma: f64,
    pub vega: f64,
    pub theta: f64,
    pub rho: f64,
    pub model_used: String,
    pub notes: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PricingError {
    /// The service indexes an empty tuple here and answers 500; the crate
    /// reports the degenerate inputs instead.
    ExpiredOrZeroVolatility,
}

impl std::fmt::Display for PricingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "time to expiry and volatility must be positive")
    }
}

/// Python prints floats without a trailing `.0` only when they have decimals,
/// so `100.0` renders as "100.0" in the notes string.
fn py_float(value: f64) -> String {
    if value.fract() == 0.0 && value.is_finite() {
        format!("{value:.1}")
    } else {
        format!("{value}")
    }
}

/// Returns price, d1, d2, delta for a vanilla option.
fn black_scholes(
    spot: f64,
    strike: f64,
    expiry: f64,
    rate: f64,
    vol: f64,
    is_call: bool,
) -> (f64, f64, f64, f64) {
    let d1 = ((spot / strike).ln() + (rate + 0.5 * vol * vol) * expiry) / (vol * expiry.sqrt());
    let d2 = d1 - vol * expiry.sqrt();
    if is_call {
        let price = spot * norm_cdf(d1) - strike * (-rate * expiry).exp() * norm_cdf(d2);
        (price, d1, d2, norm_cdf(d1))
    } else {
        let price = strike * (-rate * expiry).exp() * norm_cdf(-d2) - spot * norm_cdf(-d1);
        (price, d1, d2, -norm_cdf(-d1))
    }
}

pub fn price_derivative(req: &DerivativeRequest) -> Result<DerivativeResult, PricingError> {
    let (spot, strike, expiry, rate, vol) = (
        req.spot_price,
        req.strike_price,
        req.time_to_expiry,
        req.risk_free_rate,
        req.volatility,
    );
    if expiry <= 0.0 || vol <= 0.0 {
        return Err(PricingError::ExpiredOrZeroVolatility);
    }

    let (vanilla, d1, d2, vanilla_delta) =
        black_scholes(spot, strike, expiry, rate, vol, req.is_call);

    let n_prime_d1 = norm_pdf(d1);
    let gamma = n_prime_d1 / (spot * vol * expiry.sqrt());
    let vega = spot * expiry.sqrt() * n_prime_d1;
    let theta = (-spot * n_prime_d1 * vol / (2.0 * expiry.sqrt())
        - rate
            * strike
            * (-rate * expiry).exp()
            * if req.is_call {
                norm_cdf(d2)
            } else {
                norm_cdf(-d2)
            })
        / 365.0;
    let rho = strike
        * expiry
        * (-rate * expiry).exp()
        * if req.is_call {
            norm_cdf(d2) / 100.0
        } else {
            -norm_cdf(-d2) / 100.0
        };

    let (price, delta, model) = match req.option_type.as_str() {
        "asian" => {
            let denominator = 6.0 * (req.observations as f64 + 1.0);
            let adjusted_sigma = if denominator != 0.0 {
                vol * ((2.0 * req.observations as f64 + 1.0) / denominator).sqrt()
            } else {
                vol
            };
            let adjusted_expiry =
                expiry * (req.observations as f64 + 1.0) / (2.0 * (req.observations as f64 + 1.0));
            let (asian_price, _, _, asian_delta) = black_scholes(
                spot,
                strike,
                adjusted_expiry,
                rate,
                adjusted_sigma,
                req.is_call,
            );
            (
                asian_price,
                asian_delta,
                "Asian (arithmetic average) - adjusted Black-Scholes".to_string(),
            )
        }
        "lookback" => (
            vanilla * 1.5,
            vanilla_delta,
            "Lookback - approximate premium over vanilla".to_string(),
        ),
        "binary" => (
            strike
                * (-rate * expiry).exp()
                * if req.is_call {
                    norm_cdf(d2)
                } else {
                    norm_cdf(-d2)
                },
            vanilla_delta,
            "Binary/Digital cash-or-nothing".to_string(),
        ),
        "barrier" => {
            if req.barrier_price != 0.0 {
                let barrier_prob = norm_cdf(
                    ((req.barrier_price / spot).ln() + (rate - 0.5 * vol * vol) * expiry)
                        / (vol * expiry.sqrt()),
                );
                if req.barrier_type == "knock_out" {
                    (
                        vanilla * (1.0 - barrier_prob),
                        vanilla_delta,
                        format!("Barrier knock-out at {}", py_float(req.barrier_price)),
                    )
                } else {
                    (
                        vanilla * barrier_prob,
                        vanilla_delta,
                        format!("Barrier knock-in at {}", py_float(req.barrier_price)),
                    )
                }
            } else {
                (
                    vanilla,
                    vanilla_delta,
                    "Barrier (no barrier set - using vanilla)".to_string(),
                )
            }
        }
        _ => (vanilla, vanilla_delta, "Vanilla Black-Scholes".to_string()),
    };

    Ok(DerivativeResult {
        option_type: req.option_type.clone(),
        underlying: req.underlying.clone(),
        estimated_price: py_round(price, 4),
        delta: py_round(delta, 4),
        gamma: py_round(gamma, 6),
        vega: py_round(vega, 4),
        theta: py_round(theta, 4),
        rho: py_round(rho, 4),
        model_used: model,
        notes: format!(
            "Parameters: S={}, K={}, T={}y, sigma={}, r={}",
            py_float(spot),
            py_float(strike),
            py_float(expiry),
            py_float(vol),
            py_float(rate)
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(option_type: &str) -> DerivativeRequest {
        DerivativeRequest {
            option_type: option_type.into(),
            underlying: "ZWG/USD".into(),
            spot_price: 100.0,
            strike_price: 105.0,
            volatility: 0.25,
            risk_free_rate: 0.06,
            time_to_expiry: 1.0,
            barrier_price: 0.0,
            barrier_type: String::new(),
            is_call: true,
            observations: 12,
        }
    }

    #[test]
    fn erf_matches_the_reference_values() {
        assert!((erf(0.5) - 0.5204998778130465).abs() < 1e-12);
        assert!((erf(-1.0) - (-0.8427007929497149)).abs() < 1e-12);
        assert!((erf(2.5) - 0.999593047982555).abs() < 1e-12);
        assert!((erf(4.0) - 0.9999999845827421).abs() < 1e-12);
    }

    #[test]
    fn vanilla_prices_and_greeks_are_positive() {
        let out = price_derivative(&request("vanilla")).unwrap();
        assert_eq!(out.model_used, "Vanilla Black-Scholes");
        assert!(out.estimated_price > 0.0);
        assert!(out.gamma > 0.0 && out.vega > 0.0);
    }

    #[test]
    fn lookback_and_asian_reprice_from_the_vanilla() {
        let vanilla = price_derivative(&request("vanilla")).unwrap();
        let lookback = price_derivative(&request("lookback")).unwrap();
        let asian = price_derivative(&request("asian")).unwrap();
        assert!((lookback.estimated_price - vanilla.estimated_price * 1.5).abs() < 1e-3);
        assert_ne!(asian.estimated_price, vanilla.estimated_price);
        // greeks stay vanilla even for the exotics
        assert_eq!(asian.gamma, vanilla.gamma);
    }

    #[test]
    fn barrier_scales_by_the_touch_probability() {
        let mut req = request("barrier");
        req.barrier_price = 110.0;
        req.barrier_type = "knock_out".into();
        let out = price_derivative(&req).unwrap();
        let vanilla = price_derivative(&request("vanilla")).unwrap();
        assert!(out.estimated_price < vanilla.estimated_price);
        assert_eq!(out.model_used, "Barrier knock-out at 110.0");
    }

    #[test]
    fn notes_print_python_style_floats() {
        let out = price_derivative(&request("vanilla")).unwrap();
        assert_eq!(
            out.notes,
            "Parameters: S=100.0, K=105.0, T=1.0y, sigma=0.25, r=0.06"
        );
    }

    #[test]
    fn expired_option_is_an_error_not_a_panic() {
        let mut req = request("vanilla");
        req.time_to_expiry = 0.0;
        assert_eq!(
            price_derivative(&req),
            Err(PricingError::ExpiredOrZeroVolatility)
        );
    }
}
