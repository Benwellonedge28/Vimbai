#![forbid(unsafe_code)]
//! Shared helpers for all Vimbai calculation libraries.

/// Round a `f64` to `places` decimals using round-half-to-even on the
/// shortest decimal representation, matching Python's `round()`
/// behaviour closely enough for API-output parity.
pub fn py_round(value: f64, places: usize) -> f64 {
    // Rust's fixed-precision float formatting rounds ties-to-even on the
    // exact binary value, matching Python's round() semantics.
    format!("{:.*}", places, value).parse().unwrap_or(value)
}

/// Round to 2 decimal places (the dominant currency/report precision).
pub fn py_round2(value: f64) -> f64 {
    py_round(value, 2)
}

/// "Favorable"/"Adverse" classification used across variance services.
pub fn favorable_adverse(value: f64) -> &'static str {
    if value > 0.0 {
        "Favorable"
    } else {
        "Adverse"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounds_half_to_even_like_python() {
        assert_eq!(py_round2(2.675), 2.67); // python: round(2.675, 2) == 2.67
        assert_eq!(py_round2(1.005), 1.0); // python: round(1.005, 2) == 1.0
        assert_eq!(py_round2(2.5), 2.5);
        assert_eq!(py_round(0.125, 2), 0.12); // banker's rounding
    }

    #[test]
    fn classifies_favorable() {
        assert_eq!(favorable_adverse(5.0), "Favorable");
        assert_eq!(favorable_adverse(-0.01), "Adverse");
        assert_eq!(favorable_adverse(0.0), "Adverse");
    }
}
