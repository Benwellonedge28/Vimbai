//! The standard normal and Black-Scholes, in one place.
//!
//! This lived twice, once in the market risk crate and once in the treasury crate, with a
//! third copy of the distribution in the options module. The three agreed, which is why the
//! duplication survived review, but a fix to one of them would have left the other two
//! answering differently. One implementation now, used by both.
//!
//! `erf` is the Maclaurin series, summed in order of decreasing magnitude with a running
//! compensation. Good to about 1e-12 over the range option pricing needs (|x| up to 4);
//! beyond that the CDF is 1 or 0 to double precision anyway.

#![forbid(unsafe_code)]

/// Error function.
///
/// A Maclaurin series through the middle of the range and the asymptotic complementary
/// expansion beyond it. The series alone saturates unhelpfully near four, and a barrier
/// probability quoted to six places needs the tail to stay accurate out there.
pub fn erf(x: f64) -> f64 {
    if !x.is_finite() {
        return if x.is_nan() { f64::NAN } else { x.signum() };
    }
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
        1.0 - erfc_asymptotic(x)
    } else {
        1.0
    };
    sign * value
}

/// The complementary tail, erfc(x) = e^(-x^2)/(x sqrt(pi)) * sum (-1)^n (2n-1)!!/(2x^2)^n.
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

/// Standard normal cumulative distribution.
pub fn norm_cdf(x: f64) -> f64 {
    if x.is_nan() {
        return f64::NAN;
    }
    0.5 * (1.0 + erf(x / std::f64::consts::SQRT_2))
}

/// Standard normal density.
pub fn norm_pdf(x: f64) -> f64 {
    (-0.5 * x * x).exp() / (2.0 * std::f64::consts::PI).sqrt()
}

pub fn d1_d2(spot: f64, strike: f64, years: f64, rate: f64, volatility: f64) -> (f64, f64) {
    let d1 = ((spot / strike).ln() + (rate + volatility * volatility / 2.0) * years)
        / (volatility * years.sqrt());
    (d1, d1 - volatility * years.sqrt())
}

/// European call, Black-Scholes with no dividend.
pub fn black_scholes_call(spot: f64, strike: f64, years: f64, rate: f64, volatility: f64) -> f64 {
    let (d1, d2) = d1_d2(spot, strike, years, rate, volatility);
    spot * norm_cdf(d1) - strike * (-rate * years).exp() * norm_cdf(d2)
}

/// European put, Black-Scholes with no dividend.
pub fn black_scholes_put(spot: f64, strike: f64, years: f64, rate: f64, volatility: f64) -> f64 {
    let (d1, d2) = d1_d2(spot, strike, years, rate, volatility);
    strike * (-rate * years).exp() * norm_cdf(-d2) - spot * norm_cdf(-d1)
}

/// A string field out of a JSON payload, empty when it was never given.
///
/// This sat in the store's reader module, which is why a pure arithmetic crate ended up depending on the database
/// adapter to read one string out of a request. It is a payload helper, not storage, so it lives with the rest of the
/// shared vocabulary and the store delegates here rather than keeping a second copy.
pub fn payload_text(payload: &serde_json::Value, key: &str) -> String {
    payload
        .get(key)
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cdf_is_half_at_zero_and_saturates_at_the_tails() {
        assert!((norm_cdf(0.0) - 0.5).abs() < 1e-12);
        assert!(norm_cdf(-6.0) < 1e-9);
        assert!(norm_cdf(6.0) > 1.0 - 1e-9);
    }

    #[test]
    fn the_density_is_the_expected_height() {
        assert!((norm_pdf(0.0) - 0.398_942_280_401_432_7).abs() < 1e-12);
    }

    #[test]
    fn black_scholes_matches_the_textbook_figure() {
        // s=100, k=100, t=1, r=5%, sigma=20% -> 10.4506
        let call = black_scholes_call(100.0, 100.0, 1.0, 0.05, 0.2);
        assert!((call - 10.4506).abs() < 1e-3, "{call}");
    }

    #[test]
    fn put_call_parity_holds() {
        let (spot, strike, years, rate, sigma) = (100.0, 105.0, 1.0, 0.05, 0.2);
        let call = black_scholes_call(spot, strike, years, rate, sigma);
        let put = black_scholes_put(spot, strike, years, rate, sigma);
        let parity = call - put - (spot - strike * (-rate * years).exp());
        assert!(parity.abs() < 1e-9, "parity gap {parity}");
    }
}
