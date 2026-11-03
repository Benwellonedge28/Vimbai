//! Rounding, deliberately matching Python's `round(x, 2)`.
//!
//! The services rounded with Python's `round`, which rounds the *binary* value
//! half-to-even. Rust's fixed-precision formatting rounds the same way, so a
//! parity oracle comparing cents will agree.

/// Round to `places` decimals the way Python's `round` does for a float.
pub fn round_dp(value: f64, places: usize) -> f64 {
    if !value.is_finite() {
        return value;
    }
    format!("{:.*}", places, value)
        .parse::<f64>()
        .unwrap_or(value)
}

/// The pervasive `round(x, 2)`.
pub fn money(value: f64) -> f64 {
    round_dp(value, 2)
}

/// Render a float the way Python's `str()` does, for text that goes into a
/// journal narrative: `120000.0`, not `120000`.
pub fn python_float(value: f64) -> String {
    if !value.is_finite() {
        return format!("{value}");
    }
    if value == value.trunc() && value.abs() < 1e16 {
        format!("{value:.1}")
    } else {
        format!("{value}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_python_rounding() {
        assert_eq!(money(1.005), 1.0); // python: 1.0 (binary value is just under)
        assert_eq!(money(2.675), 2.67);
        assert_eq!(money(2.5), 2.5);
        assert_eq!(money(0.125), 0.12); // half-to-even at 2dp: 0.12
        assert_eq!(money(0.135), 0.14);
        assert_eq!(round_dp(1.23456, 4), 1.2346);
        assert_eq!(money(-0.0), -0.0);
    }
}
