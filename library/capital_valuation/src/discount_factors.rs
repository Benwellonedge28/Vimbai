//! Discount factors, from discount-factor-service.
//!
//! The table endpoints round for display the way the Python service does: six places for
//! the factor, four for the rate. Formatting a double and parsing it back gives the same
//! tie-to-even answer Python's round() gives, which plain multiplication and truncation
//! would not.

#![forbid(unsafe_code)]

use serde_json::{json, Value};

fn number(payload: &Value, key: &str) -> Result<f64, String> {
    match payload.get(key) {
        Some(Value::Number(value)) => value
            .as_f64()
            .ok_or_else(|| format!("{key} is not a number")),
        Some(Value::String(text)) => text
            .trim()
            .parse::<f64>()
            .map_err(|_| format!("{key} is not a number")),
        Some(other) => Err(format!("{key} is {other}, expected a number")),
        None => Err(format!("{key} is required")),
    }
}

fn years(payload: &Value, key: &str) -> Result<i64, String> {
    let raw = number(payload, key)?;
    Ok(raw.trunc() as i64)
}

fn rounded(value: f64, places: usize) -> f64 {
    format!("{value:.places$}").parse::<f64>().unwrap_or(value)
}

fn factor(rate: f64, years: i64) -> f64 {
    1.0 / (1.0 + rate).powf(years as f64)
}

fn annuity_factor(rate: f64, years: i64) -> f64 {
    if rate == 0.0 {
        years as f64
    } else {
        (1.0 - (1.0 / (1.0 + rate).powf(years as f64))) / rate
    }
}

/// `/calculate` — rate as a decimal.
pub fn calculate_from_json(payload: &Value) -> Result<Value, String> {
    let rate = number(payload, "rate")?;
    let years = years(payload, "years")?;
    if years < 0 {
        return Ok(json!({"error": "Years cannot be negative"}));
    }
    Ok(json!({
        "rate": rate,
        "years": years,
        "discount_factor": factor(rate, years),
    }))
}

/// `/calculate-percent` — rate given as a percentage.
pub fn calculate_percent_from_json(payload: &Value) -> Result<Value, String> {
    let rate_percent = number(payload, "rate_percent")?;
    let years = years(payload, "years")?;
    let rate_decimal = rate_percent / 100.0;
    Ok(json!({
        "rate_percent": rate_percent,
        "rate_decimal": rate_decimal,
        "years": years,
        "discount_factor": factor(rate_decimal, years),
    }))
}

/// `/annuity-factor`.
pub fn annuity_factor_from_json(payload: &Value) -> Result<Value, String> {
    let rate = number(payload, "rate")?;
    let years = years(payload, "years")?;
    let pvaf = annuity_factor(rate, years);
    Ok(json!({
        "rate": rate,
        "years": years,
        "annuity_factor": pvaf,
        "formula": format!("[1 - 1/(1+{rate})^{years}]/{rate} = {pvaf}"),
    }))
}

/// `/annuity-factor-percent`.
pub fn annuity_factor_percent_from_json(payload: &Value) -> Result<Value, String> {
    let rate_percent = number(payload, "rate_percent")?;
    let years = years(payload, "years")?;
    let rate_decimal = rate_percent / 100.0;
    Ok(json!({
        "rate_percent": rate_percent,
        "rate_decimal": rate_decimal,
        "years": years,
        "annuity_factor": annuity_factor(rate_decimal, years),
    }))
}

/// `/table` — one rate across a list of years.
pub fn table_from_json(payload: &Value) -> Result<Value, String> {
    let base_rate = number(payload, "base_rate")?;
    let list = payload
        .get("years")
        .and_then(Value::as_array)
        .ok_or("years must be a list of integers")?;
    let mut factors = Vec::with_capacity(list.len());
    for item in list {
        let year = match item {
            Value::Number(value) => value.as_i64().ok_or("years holds a non-integer")?,
            Value::String(text) => text
                .trim()
                .parse::<i64>()
                .map_err(|_| "years holds a non-integer")?,
            _ => return Err("years holds a non-integer".to_string()),
        };
        factors.push(json!({
            "year": year,
            "discount_factor": rounded(factor(base_rate, year), 6),
        }));
    }
    Ok(json!({"rate": base_rate, "factors": factors}))
}

/// `/table-range` — a rate range at one year count.
///
/// The rate walks forward by repeated addition, exactly as the service does, so the two
/// agree on where the accumulation lands rather than only on the arithmetic.
pub fn table_range_from_json(payload: &Value) -> Result<Value, String> {
    let rate_start = number(payload, "rate_start")?;
    let rate_end = number(payload, "rate_end")?;
    let rate_step = number(payload, "rate_step")?;
    let years = years(payload, "years")?;

    let mut factors = Vec::new();
    let mut rate = rate_start;
    while rate <= rate_end {
        factors.push(json!({
            "rate": rounded(rate, 4),
            "discount_factor": rounded(factor(rate, years), 6),
        }));
        rate += rate_step;
        if factors.len() > 10_000 {
            break;
        }
    }
    Ok(json!({"years": years, "factors": factors}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discount_factor_matches_the_formula() {
        let out = calculate_from_json(&json!({"rate": 0.1, "years": 3})).unwrap();
        let df = out["discount_factor"].as_f64().unwrap();
        assert!((df - 0.7513148009015778).abs() < 1e-12, "{df}");
    }

    #[test]
    fn a_percentage_rate_is_the_decimal_rate() {
        let decimal = calculate_from_json(&json!({"rate": 0.1, "years": 5})).unwrap();
        let percent =
            calculate_percent_from_json(&json!({"rate_percent": 10.0, "years": 5})).unwrap();
        assert_eq!(decimal["discount_factor"], percent["discount_factor"]);
    }

    #[test]
    fn a_zero_rate_annuity_factor_is_the_year_count() {
        let out = annuity_factor_from_json(&json!({"rate": 0.0, "years": 7})).unwrap();
        assert_eq!(out["annuity_factor"], json!(7.0));
    }

    #[test]
    fn negative_years_are_refused_the_way_the_service_refuses_them() {
        let out = calculate_from_json(&json!({"rate": 0.1, "years": -1})).unwrap();
        assert_eq!(out["error"], json!("Years cannot be negative"));
    }

    #[test]
    fn the_table_rounds_the_way_the_service_rounds() {
        let out = table_from_json(&json!({"base_rate": 0.1, "years": [1, 2]})).unwrap();
        assert_eq!(out["factors"][0]["discount_factor"], json!(0.909091));
        assert_eq!(out["factors"][1]["discount_factor"], json!(0.826446));
    }

    #[test]
    fn the_rate_range_stops_where_repeated_addition_stops() {
        // 0.05 + 0.05 + 0.05 lands slightly above 0.15 as a double, so the walk stops at
        // two rows. The Python service does the same, and a "fixed" inclusive loop here
        // would quietly return a row it never returned.
        let out = table_range_from_json(&json!({
            "rate_start": 0.05,
            "rate_end": 0.15,
            "rate_step": 0.05,
            "years": 2
        }))
        .unwrap();
        assert_eq!(out["factors"].as_array().unwrap().len(), 2);
        assert_eq!(out["factors"][1]["rate"], json!(0.1));
    }

    #[test]
    fn a_step_that_divides_cleanly_reaches_the_end() {
        let out = table_range_from_json(&json!({
            "rate_start": 0.02,
            "rate_end": 0.06,
            "rate_step": 0.02,
            "years": 3
        }))
        .unwrap();
        assert_eq!(out["factors"].as_array().unwrap().len(), 3);
    }
}
