#![forbid(unsafe_code)]
//! internal-rate-return-service: the rate at which a project's flows are worth exactly what it costs.
//!
//! The rate is found by iteration, so this is the one place in the crate that searches rather than computes. The
//! search runs in a float because a root is an approximation by nature; what it settles on is then priced exactly
//! through the shared engine, and that exact net is what the answer reports, so a caller can see how close to zero
//! the rate really came.
//!
//! Behaviours kept as shipped, each pinned by a test:
//!
//! * an empty series or an investment of nothing is refused rather than guessed at;
//! * the search starts from the average flow over the investment, which is the original's first guess;
//! * a rate below -0.99 is reset to -0.5 and one above 10 to 5, so a wild step cannot run away;
//! * the returned percentage is the decimal times a hundred only when the decimal is positive, which is how the
//!   original published a negative rate (as a decimal, not a percentage).
//!
//! One correction: the original's search was not checked against the net it produced, and it could settle on a rate
//! where the net was nowhere near zero - a project paying back 1100 for 1000 inside one year, which is ten percent,
//! came back as -0.81. The rate is now verified against the exact net, and when it does not hold the root is found
//! again by halving a bracket that changes sign, which is the case for any project whose flows cross zero once.
//! * the decision compares the rate with the required rate and reports the gap between them.

use serde_json::{json, Value};

use vimbai_common::Money;
use vimbai_store::read::{decimal, refused};
use vimbai_store::StoreError;

use crate::discounting::{flows, investment, net_present_value, residual};

/// What the series is worth today at this rate, as a float, for the search only.
fn net_at(invested: f64, series: &[f64], at: f64, residual_value: f64) -> f64 {
    let mut total = 0.0;
    for (index, flow) in series.iter().enumerate() {
        total += flow / (1.0 + at).powi(index as i32 + 1);
    }
    if residual_value > 0.0 {
        total += residual_value / (1.0 + at).powi(series.len() as i32);
    }
    total - invested
}

fn as_floats(series: &[Money]) -> Vec<f64> {
    series.iter().map(Money::to_f64_lossy).collect()
}

/// How far from zero a net may sit and still count as a root: a hundredth of a millionth of the money at stake.
fn close_enough(net: f64, invested: f64) -> bool {
    net.abs() <= invested.abs().max(1.0) * 1e-8
}

/// The rate the flows earn: the faster search first, then a bracket when the faster one did not land on a root.
fn converge(
    invested: f64,
    series: &[f64],
    residual_value: f64,
    iterations: u32,
    tolerance: f64,
) -> f64 {
    let mut at = series.iter().sum::<f64>() / series.len() as f64 / invested;

    for _ in 0..iterations {
        let net = net_at(invested, series, at, residual_value);
        let delta = 0.0001;
        let ahead = net_at(invested, series, at + delta, residual_value);
        let slope = (ahead - net) / delta;
        if slope.abs() < 1e-10 {
            break;
        }
        let next = at - net / slope;
        if (next - at).abs() < tolerance {
            at = next;
            break;
        }
        at = next;
        if at < -0.99 {
            at = -0.5;
        } else if at > 10.0 {
            at = 5.0;
        }
    }

    if close_enough(net_at(invested, series, at, residual_value), invested) {
        return at;
    }
    bracket(invested, series, residual_value).unwrap_or(at)
}

/// A root between -0.99 and ten, found by halving the first interval that changes sign.
fn bracket(invested: f64, series: &[f64], residual_value: f64) -> Option<f64> {
    let step = 0.01;
    let mut low = -0.99;
    let mut low_net = net_at(invested, series, low, residual_value);
    while low < 10.0 {
        let high = low + step;
        let high_net = net_at(invested, series, high, residual_value);
        if low_net * high_net <= 0.0 {
            let (mut left, mut right) = (low, high);
            for _ in 0..200 {
                let middle = (left + right) / 2.0;
                let middle_net = net_at(invested, series, middle, residual_value);
                if close_enough(middle_net, invested) {
                    return Some(middle);
                }
                if low_net * middle_net < 0.0 {
                    right = middle;
                } else {
                    left = middle;
                }
            }
            return Some((left + right) / 2.0);
        }
        low = high;
        low_net = high_net;
    }
    None
}

/// `POST /calculate`: the rate found by Newton-Raphson.
pub fn calculate(payload: &Value) -> Result<Value, StoreError> {
    let invested = investment(payload)?;
    let series = flows(payload, "cash_flows")?;
    let residual_value = residual(payload)?;
    let iterations = if payload.get("max_iterations").is_some() {
        vimbai_store::read::whole(payload, "max_iterations").clamp(1, 1000)
    } else {
        100
    };
    let tolerance = decimal(payload, "tolerance", "0.0001")
        .parse::<f64>()
        .map_err(|_| refused("tolerance must be a number"))?;

    let invested_f = invested.to_f64_lossy();
    let series_f = as_floats(&series);
    let at = converge(
        invested_f,
        &series_f,
        residual_value.to_f64_lossy(),
        iterations,
        tolerance,
    );

    // verified exactly at the rate the search settled on
    let as_money = Money::from_f64_shortest(at).map_err(|_| refused("the search left the rate"))?;
    let gap = net_present_value(&invested, &series, &as_money, &residual_value);
    let percentage = if at > 0.0 { at * 100.0 } else { at };

    Ok(json!({
        "initial_investment": invested.round(2).to_fixed(2),
        "cash_flows": series.iter().map(|flow| flow.round(2).to_fixed(2)).collect::<Vec<_>>(),
        "residual_value": residual_value.round(2).to_fixed(2),
        "irr_percentage": format!("{:.2}", percentage),
        "irr_decimal": as_money.round(6).to_fixed(6),
        "npv_at_irr": gap.round(2).to_fixed(2),
        "formula": "IRR is the rate where NPV = 0",
    }))
}

/// `POST /calculate-bisection`: the same rate found by halving a bracket instead.
pub fn calculate_bisection(payload: &Value) -> Result<Value, StoreError> {
    let invested = investment(payload)?;
    let series = flows(payload, "cash_flows")?;
    let residual_value = residual(payload)?;
    let mut low = decimal(payload, "low_rate", "0")
        .parse::<f64>()
        .map_err(|_| refused("low_rate must be a number"))?;
    let mut high = decimal(payload, "high_rate", "1")
        .parse::<f64>()
        .map_err(|_| refused("high_rate must be a number"))?;
    let iterations = if payload.get("iterations").is_some() {
        vimbai_store::read::whole(payload, "iterations").clamp(1, 1000)
    } else {
        50
    };
    let tolerance = decimal(payload, "tolerance", "0.0001")
        .parse::<f64>()
        .map_err(|_| refused("tolerance must be a number"))?;

    let invested_f = invested.to_f64_lossy();
    let series_f = as_floats(&series);
    let residual_f = residual_value.to_f64_lossy();
    let mut middle = (low + high) / 2.0;

    for _ in 0..iterations {
        middle = (low + high) / 2.0;
        let net = net_at(invested_f, &series_f, middle, residual_f);
        if net.abs() < tolerance {
            break;
        }
        if net_at(invested_f, &series_f, low, residual_f) * net < 0.0 {
            high = middle;
        } else {
            low = middle;
        }
    }

    let as_money =
        Money::from_f64_shortest(middle).map_err(|_| refused("the search left the rate"))?;
    let gap = net_present_value(&invested, &series, &as_money, &residual_value);

    Ok(json!({
        "initial_investment": invested.round(2).to_fixed(2),
        "irr_percentage": format!("{:.2}", middle * 100.0),
        "npv_at_calculated_irr": gap.round(2).to_fixed(2),
    }))
}

/// `POST /compare`: the best of several projects by the rate they earn.
pub fn compare(payload: &Value) -> Result<Value, StoreError> {
    let projects = payload
        .get("projects")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let mut ranked: Vec<(String, f64)> = Vec::new();
    for project in projects {
        let name = project
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("Unknown")
            .to_string();
        let (Ok(invested), Ok(series)) = (investment(&project), flows(&project, "cash_flows"))
        else {
            continue;
        };
        let residual_value = residual(&project).unwrap_or_else(|_| Money::zero());
        let found = calculate(&json!({
            "initial_investment": invested.to_fixed(6),
            "cash_flows": series.iter().map(|flow| flow.to_fixed(6)).collect::<Vec<_>>(),
            "residual_value": residual_value.to_fixed(6),
        }))?;
        let percentage = found["irr_percentage"]
            .as_str()
            .unwrap_or("0")
            .parse::<f64>()
            .unwrap_or(0.0);
        ranked.push((name, percentage));
    }

    ranked.sort_by(|one, two| {
        two.1
            .partial_cmp(&one.1)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let rows: Vec<Value> = ranked
        .iter()
        .map(|(name, percentage)| json!({"name": name, "irr_percentage": format!("{percentage:.2}")}))
        .collect();
    let recommended = ranked.first().map(|(name, _)| name.clone());

    Ok(json!({
        "project_comparison": rows,
        "recommended_project": recommended,
    }))
}

/// `POST /decision`: does the rate clear the rate required.
pub fn decision(payload: &Value) -> Result<Value, StoreError> {
    let earned =
        Money::parse(&decimal(payload, "irr", "0")).map_err(|_| refused("irr must be a rate"))?;
    let required = Money::parse(&decimal(payload, "required_rate", "0"))
        .map_err(|_| refused("required_rate must be a rate"))?;

    let verdict = match earned.as_decimal().cmp(required.as_decimal()) {
        std::cmp::Ordering::Greater => "Accept",
        std::cmp::Ordering::Less => "Reject",
        std::cmp::Ordering::Equal => "Indifferent",
    };

    Ok(json!({
        "irr": earned.round(4).to_fixed(4),
        "required_rate": required.round(4).to_fixed(4),
        "decision": verdict,
        "difference": earned.minus(&required).round(4).to_fixed(4),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_project_returning_exactly_ten_percent_is_found_as_ten_percent() {
        // 1000 in, 1100 out after one year, is ten percent
        let answer =
            calculate(&json!({"initial_investment": 1000, "cash_flows": [1100]})).expect("found");
        assert_eq!(answer["irr_percentage"], json!("10.00"));
        assert_eq!(answer["npv_at_irr"], json!("0.00"));
    }

    #[test]
    fn a_rate_found_over_several_years_leaves_a_net_of_nothing() {
        let answer = calculate(&json!({
            "initial_investment": 1000, "cash_flows": [500, 500, 500]
        }))
        .expect("found");
        // about 23.4 percent, and the exact net at that rate is flat to the cent
        assert!(answer["irr_percentage"]
            .as_str()
            .unwrap()
            .starts_with("23."));
        assert_eq!(answer["npv_at_irr"], json!("0.00"));
    }

    #[test]
    fn an_empty_series_or_a_zero_investment_is_refused() {
        assert!(calculate(&json!({"initial_investment": 1000, "cash_flows": []})).is_err());
        assert!(calculate(&json!({"initial_investment": 0, "cash_flows": [100]})).is_err());
    }

    #[test]
    fn bisection_finds_the_same_rate_as_newton_raphson() {
        let seed = json!({"initial_investment": 1000, "cash_flows": [1100]});
        let found = calculate(&seed).expect("found");
        let halved = calculate_bisection(&json!({
            "initial_investment": 1000, "cash_flows": [1100], "low_rate": 0, "high_rate": 1
        }))
        .expect("found");
        assert_eq!(found["irr_percentage"], halved["irr_percentage"]);
    }

    #[test]
    fn comparison_puts_the_rate_that_clears_most_first() {
        let answer = compare(&json!({"projects": [
            {"name": "Ten", "initial_investment": 1000, "cash_flows": [1100]},
            {"name": "Twenty", "initial_investment": 1000, "cash_flows": [1200]},
            {"name": "Nothing", "initial_investment": 1000, "cash_flows": []},
        ]}))
        .expect("compared");
        assert_eq!(answer["recommended_project"], json!("Twenty"));
        assert_eq!(answer["project_comparison"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn the_decision_reports_the_gap_to_the_rate_required() {
        let answer = decision(&json!({"irr": 12, "required_rate": 10})).expect("decided");
        assert_eq!(answer["decision"], json!("Accept"));
        assert_eq!(answer["difference"], json!("2.0000"));

        let short = decision(&json!({"irr": 8, "required_rate": 10})).expect("decided");
        assert_eq!(short["decision"], json!("Reject"));
    }
}
