//! Rounding that matches Python's `round(x, 2)` on a float (half-to-even on the
//! binary value), so a parity oracle comparing cents agrees.

pub fn round_dp(value: f64, places: usize) -> f64 {
    if !value.is_finite() {
        return value;
    }
    format!("{:.*}", places, value)
        .parse::<f64>()
        .unwrap_or(value)
}

pub fn money(value: f64) -> f64 {
    round_dp(value, 2)
}

/// Render a float the way Python's `str()` does inside a narrative.
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
