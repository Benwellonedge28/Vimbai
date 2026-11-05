//! Small numeric helpers, in one place.
//!
//! These lived four times over, identical in every copy, in the group accounting, market
//! risk, performance and tax crates. Four copies of a rounding rule is four chances for one
//! of them to drift, and money is the last place to want that.

#![forbid(unsafe_code)]

/// Round to a number of decimal places, ties to even, the way Python's `round` does.
pub fn round_dp(value: f64, places: usize) -> f64 {
    if !value.is_finite() {
        return value;
    }
    format!("{value:.places$}").parse::<f64>().unwrap_or(value)
}

/// Round to currency precision.
pub fn money(value: f64) -> f64 {
    round_dp(value, 2)
}

/// Render a float the way Python's `str()` does inside an f-string narrative.
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
    fn rounding_is_ties_to_even_like_python() {
        assert_eq!(round_dp(2.675, 2), 2.67);
        assert_eq!(money(1.005), 1.0);
        assert_eq!(money(1.015), 1.01);
    }

    #[test]
    fn python_float_keeps_a_trailing_point_on_whole_numbers() {
        assert_eq!(python_float(12.0), "12.0");
        assert_eq!(python_float(12.5), "12.5");
    }
}
