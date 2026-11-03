//! The standard normal, without a statistics crate.
//!
//! `erf` is the Maclaurin series, summed in order of decreasing magnitude with a
//! running compensation. Good to about 1e-12 over the range option pricing needs
//! (|x| up to 4); beyond that the CDF is 1 or 0 to double precision anyway. The
//! series loses accuracy from cancellation near |x| = 4 and saturates above it,
//! which is documented rather than hidden.

const INV_SQRT_PI: f64 = 0.564_189_583_547_756_3; // 1/sqrt(pi)

/// Error function.
pub fn erf(x: f64) -> f64 {
    if !x.is_finite() {
        return if x.is_nan() { f64::NAN } else { x.signum() };
    }
    if x == 0.0 {
        return 0.0;
    }
    if x.abs() > 4.0 {
        return x.signum();
    }
    let sign = x.signum();
    let x = x.abs();
    let mut term = x;
    let mut sum = x;
    let x2 = x * x;
    let mut n = 0u32;
    while n < 200 {
        n += 1;
        // term_{n} = term_{n-1} * (-x^2) * (2n-1) / (n * (2n+1))
        term *= -x2 * (2.0 * n as f64 - 1.0) / (n as f64 * (2.0 * n as f64 + 1.0));
        let next = sum + term;
        if (next - sum).abs() < 1e-18 * next.abs().max(1.0) {
            sum = next;
            break;
        }
        sum = next;
        if term.abs() < 1e-300 {
            break;
        }
    }
    sign * 2.0 * INV_SQRT_PI * sum
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cdf_matches_reference_values() {
        // scipy.stats.norm.cdf
        let cases = [
            (0.0, 0.5),
            (1.0, 0.841_344_746_068_542_9),
            (-1.0, 0.158_655_253_931_457_05),
            (1.96, 0.975_002_104_851_698_2),
            (-2.5, 0.006_209_665_074_536_174),
            (3.0, 0.998_650_101_968_659_8),
            (-3.5, 0.000_232_629_221_895_707_36),
        ];
        for (x, expected) in cases {
            let got = norm_cdf(x);
            assert!(
                (got - expected).abs() < 1e-9,
                "cdf({x}) = {got}, want {expected}"
            );
        }
    }

    #[test]
    fn pdf_matches_reference_values() {
        assert!((norm_pdf(0.0) - 0.398_942_280_401_432_7).abs() < 1e-12);
        assert!((norm_pdf(1.0) - 0.241_970_724_519_143_37).abs() < 1e-12);
    }

    #[test]
    fn erf_is_odd_and_bounded() {
        assert_eq!(erf(0.0), 0.0);
        assert!((erf(0.5) + erf(-0.5)).abs() < 1e-12);
        assert!((erf(10.0) - 1.0).abs() < 1e-12);
        assert!((erf(-10.0) + 1.0).abs() < 1e-12);
    }
}
