#![forbid(unsafe_code)]
//! The one discounting engine four capital-budgeting services share.
//!
//! net-present-value, internal-rate-return, payback-period and continue-shutdown all asked the same question
//! about the same series: what are these future amounts worth today at this rate. The arithmetic lives here, once,
//! so a change to how a discount factor is worked out reaches every one of them.
//!
//! Amounts are exact. A discount factor is a division, so it is carried to thirty-four significant decimals before
//! it is applied, and a resulting value is rounded to the cent only when it is published. The one place a float is
//! used at all is the root search behind the internal rate of return, because a root found by iteration is an
//! approximation by nature; its answer is verified exactly afterwards.

use serde_json::{json, Value};
use vimbai_common::Money;
use vimbai_store::read::{amounts, decimal, money_or_nothing, refused};
use vimbai_store::StoreError;

/// The rate as a decimal: `0.1` for ten percent. A rate of -1 or below cannot be discounted at.
pub fn rate(payload: &Value, key: &str) -> Result<Money, StoreError> {
    let rate = Money::parse(&decimal(payload, key, "0"))
        .map_err(|_| refused(&format!("{key} must be a rate")))?;
    let floor = Money::parse("-1").map_err(|_| refused("a rate must be a number"))?;
    if rate.as_decimal() <= floor.as_decimal() {
        return Err(refused(&format!("{key} must be above -1")));
    }
    Ok(rate)
}

/// The series, refused when it is empty: every one of these services needs at least one year.
pub fn flows(payload: &Value, key: &str) -> Result<Vec<Money>, StoreError> {
    let series = amounts(payload, key)?;
    if series.is_empty() {
        return Err(refused(&format!("{key} is required")));
    }
    Ok(series)
}

/// What one amount `years` from now is worth today, compounded exactly.
pub fn worth_today(amount: &Money, discount_rate: &Money, years: i64) -> Money {
    let growth = Money::from_i64(1).plus(discount_rate);
    if growth.is_zero() {
        return Money::zero();
    }
    amount.over(&growth.pow(years.max(0) as u32))
}

/// The discount factor itself, for the schedules these services publish on four places.
pub fn factor(discount_rate: &Money, years: i64) -> Money {
    let growth = Money::from_i64(1).plus(discount_rate);
    if growth.is_zero() {
        return Money::zero();
    }
    Money::from_i64(1).over(&growth.pow(years.max(0) as u32))
}

/// The worth today of every flow in the series, with an optional residual in the final year.
pub fn inflows(series: &[Money], discount_rate: &Money, residual: &Money) -> Money {
    let mut total = Money::zero();
    for (index, flow) in series.iter().enumerate() {
        total = total.plus(&worth_today(flow, discount_rate, index as i64 + 1));
    }
    if !residual.is_zero() && !residual.is_negative() {
        total = total.plus(&worth_today(residual, discount_rate, series.len() as i64));
    }
    total
}

/// What the series is worth today once the investment is paid for.
pub fn net_present_value(
    investment: &Money,
    series: &[Money],
    discount_rate: &Money,
    residual: &Money,
) -> Money {
    inflows(series, discount_rate, residual).minus(investment)
}

/// The residual from the payload, absent meaning none.
pub fn residual(payload: &Value) -> Result<Money, StoreError> {
    money_or_nothing(payload, "residual_value")
}

/// The investment from the payload, refused when it is missing altogether.
pub fn investment(payload: &Value) -> Result<Money, StoreError> {
    let amount = money_or_nothing(payload, "initial_investment")?;
    if amount.is_zero() {
        return Err(refused("initial_investment cannot be zero"));
    }
    Ok(amount)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn m(text: &str) -> Money {
        Money::parse(text).expect("an amount")
    }

    #[test]
    fn a_flow_one_year_out_is_discounted_once() {
        let value = worth_today(&m("110"), &m("0.1"), 1);
        assert_eq!(value.round(2).to_fixed(2), "100.00");
    }

    #[test]
    fn a_flow_three_years_out_is_discounted_three_times() {
        let value = worth_today(&m("133.1"), &m("0.1"), 3);
        assert_eq!(value.round(2).to_fixed(2), "100.00");
    }

    #[test]
    fn the_inflows_carry_a_residual_in_its_final_year() {
        let series = vec![m("100"), m("100")];
        let without = inflows(&series, &m("0.1"), &m("0"));
        let with = inflows(&series, &m("0.1"), &m("121"));
        assert_eq!(with.round(2).to_fixed(2), "273.55");
        assert_eq!(without.round(2).to_fixed(2), "173.55");
    }

    #[test]
    fn a_rate_of_minus_one_is_refused_rather_than_dividing_by_nothing() {
        assert!(rate(&json!({"discount_rate": "-1"}), "discount_rate").is_err());
        assert!(rate(&json!({"discount_rate": "0.08"}), "discount_rate").is_ok());
    }

    #[test]
    fn an_empty_series_is_refused() {
        assert!(flows(&json!({"cash_flows": []}), "cash_flows").is_err());
        assert!(flows(&json!({"cash_flows": [1, 2]}), "cash_flows").is_ok());
    }
}

/// `/npv` on the realtime engine: the series worth today once the investment is paid for.
///
/// The engine published its rate as a percent, so a caller saying `7.5` meant seven and a half percent. That is the one
/// difference from the capital-budgeting services, which take a rate as a decimal; the discounting itself is this
/// crate's, so there is no second NPV in the workspace.
pub fn quick_npv(payload: &Value) -> Result<Value, StoreError> {
    let investment = investment(payload)?;
    let series = flows(payload, "cash_flows")?;
    let percent = rate(payload, "discount_rate")?;
    let fraction = percent.over(&Money::from_i64(100));
    let net = net_present_value(&investment, &series, &fraction, &Money::zero());
    Ok(json!({
        "npv": net.round(2).to_f64_lossy(),
        "profitable": !net.is_zero() && !net.is_negative(),
        "rate": percent.to_f64_lossy(),
    }))
}

#[cfg(test)]
mod quick_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_realtime_engine_takes_its_rate_as_a_percent() {
        let answer = quick_npv(
            &json!({"initial_investment": 1000, "cash_flows": [500, 500, 500], "discount_rate": 10}),
        )
        .expect("a net");
        // 500 discounted at ten percent for three years is 1243.43, so the project is worth 243.43 today
        assert_eq!(answer["npv"], json!(243.43));
        assert_eq!(answer["profitable"], json!(true));
        assert_eq!(answer["rate"], json!(10.0));

        // a rate high enough to make the same flows worth less than they cost
        let dear = quick_npv(
            &json!({"initial_investment": 1000, "cash_flows": [300, 300, 300], "discount_rate": 40}),
        )
        .expect("a net");
        assert_eq!(dear["profitable"], json!(false));
        assert!(dear["npv"].as_f64().unwrap() < 0.0);

        // a series of nothing is refused, as is an investment of nothing
        assert!(quick_npv(
            &json!({"initial_investment": 1000, "cash_flows": [], "discount_rate": 10})
        )
        .is_err());
        assert!(quick_npv(
            &json!({"initial_investment": 0, "cash_flows": [10], "discount_rate": 10})
        )
        .is_err());
    }
}
