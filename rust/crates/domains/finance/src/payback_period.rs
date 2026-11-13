#![forbid(unsafe_code)]
//! payback-period-service: how long a project takes to give back what it cost.
//!
//! Two shapes, as the original had: a level series is one division, and a series that moves is walked year by year
//! until the cumulative covers the investment, with the part-year interpolated rather than rounded up to the year.
//!
//! Behaviours kept as shipped, each pinned by a test:
//!
//! * a level series pays back investment over flow; a flow of nothing or less is refused rather than divided by;
//! * a series that never covers the investment reports no period at all and says so, rather than a figure;
//! * the payback is published to two places and also as a sentence of years and months;
//! * a comparison ranks the quickest first and leaves out any project that never pays back;
//! * the average-flow route divides the total by the years and then the investment by that average, which is the
//!   same division the level series does when the years are equal.

use serde_json::{json, Value};

use vimbai_common::Money;
use vimbai_store::read::{decimal, refused, whole_signed};
use vimbai_store::StoreError;

use crate::discounting::flows;

/// "3 years and 4.0 months", which is how the original read a fractional period out loud.
fn spoken(period: &Money) -> String {
    let years = period.truncate();
    let year_count = years.to_fixed(0);
    let months = period
        .minus(&years)
        .times(&Money::parse("12").unwrap_or_else(|_| Money::zero()));
    format!("{year_count} years and {:.1} months", months.to_f64_lossy())
}

/// How long until the cumulative covers the investment, and whether it ever does.
fn walk(invested: &Money, series: &[Money]) -> Option<(Money, usize)> {
    let mut cumulative = Money::zero();
    for (index, flow) in series.iter().enumerate() {
        cumulative = cumulative.plus(flow);
        if cumulative.as_decimal() >= invested.as_decimal() {
            let before = cumulative.minus(flow);
            let remaining = invested.minus(&before);
            let part = if flow.is_zero() {
                Money::zero()
            } else {
                remaining.over(flow)
            };
            return Some((Money::from_i64(index as i64).plus(&part), index + 1));
        }
    }
    None
}

/// `POST /calculate`.
pub fn calculate(payload: &Value) -> Result<Value, StoreError> {
    let invested = Money::parse(&decimal(payload, "initial_investment", "0"))
        .map_err(|_| refused("initial_investment must be an amount"))?;
    if invested.is_zero() {
        return Err(refused("initial_investment cannot be zero"));
    }
    let series = flows(payload, "annual_cash_flows")?;

    // a level series is one division, which is what the original did before walking anything
    let level = series.iter().all(|flow| flow == &series[0]);
    if level {
        if series[0].is_negative() || series[0].is_zero() {
            return Err(refused(
                "Annual cash flow must be positive for payback calculation",
            ));
        }
        let period = invested.over(&series[0]);
        return Ok(json!({
            "initial_investment": invested.round(2).to_fixed(2),
            "annual_cash_flows": series.iter().map(|flow| flow.round(2).to_fixed(2)).collect::<Vec<_>>(),
            "equal_cash_flows": true,
            "payback_period": period.round(2).to_fixed(2),
            "payback_years": spoken(&period),
        }));
    }

    let schedule: Vec<Value> = {
        let mut running = Money::zero();
        series
            .iter()
            .enumerate()
            .map(|(index, flow)| {
                running = running.plus(flow);
                json!({
                    "year": index + 1,
                    "cash_flow": flow.round(2).to_fixed(2),
                    "cumulative": running.round(2).to_fixed(2),
                })
            })
            .collect()
    };

    match walk(&invested, &series) {
        Some((period, year)) => Ok(json!({
            "initial_investment": invested.round(2).to_fixed(2),
            "cash_flow_schedule": schedule,
            "equal_cash_flows": false,
            "payback_period": period.round(2).to_fixed(2),
            "payback_years": spoken(&period),
            "payback_achieved_in_year": year,
        })),
        None => Ok(json!({
            "initial_investment": invested.round(2).to_fixed(2),
            "cash_flow_schedule": schedule,
            "equal_cash_flows": false,
            "payback_period": Value::Null,
            "message": "Payback not achieved within given cash flow period",
        })),
    }
}

/// `POST /compare`: the quickest to pay back first.
pub fn compare(payload: &Value) -> Result<Value, StoreError> {
    let projects = payload
        .get("projects")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let mut ranked: Vec<(String, Money)> = Vec::new();
    for project in projects {
        let name = project
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("Unknown")
            .to_string();
        let invested = Money::parse(&decimal(&project, "initial_investment", "0"))
            .unwrap_or_else(|_| Money::zero());
        let series = flows(&project, "cash_flows").unwrap_or_default();
        if series.is_empty() || invested.is_zero() {
            continue;
        }

        let level = series.iter().all(|flow| flow == &series[0]);
        let period = if level {
            if series[0].is_zero() {
                None
            } else {
                Some(invested.over(&series[0]))
            }
        } else {
            walk(&invested, &series).map(|(found, _year)| found)
        };
        if let Some(found) = period {
            ranked.push((name, found));
        }
    }

    ranked.sort_by(|one, two| one.1.as_decimal().cmp(two.1.as_decimal()));
    let rows: Vec<Value> = ranked
        .iter()
        .map(|(name, period)| json!({"name": name, "payback_period": period.round(2).to_fixed(2)}))
        .collect();
    let recommended = ranked.first().map(|(name, _)| name.clone());

    Ok(json!({
        "project_comparison": rows,
        "recommended_project": recommended,
    }))
}

/// `POST /average-cash-flow`: the total spread evenly, then the investment over that average.
pub fn average_cash_flow(payload: &Value) -> Result<Value, StoreError> {
    let invested = Money::parse(&decimal(payload, "initial_investment", "0"))
        .map_err(|_| refused("initial_investment must be an amount"))?;
    let total = Money::parse(&decimal(payload, "total_cash_flows", "0"))
        .map_err(|_| refused("total_cash_flows must be an amount"))?;
    let years = whole_signed(payload, "years")?;
    if years <= 0 {
        return Err(refused("Years must be positive"));
    }
    if invested.is_zero() {
        return Err(refused("initial_investment cannot be zero"));
    }

    let average = total.over(&Money::from_i64(years));
    let period = if average.is_negative() || average.is_zero() {
        Value::Null
    } else {
        json!(invested.over(&average).round(2).to_fixed(2))
    };

    Ok(json!({
        "initial_investment": invested.round(2).to_fixed(2),
        "total_cash_flows": total.round(2).to_fixed(2),
        "years": years,
        "average_annual_cash_flow": average.round(2).to_fixed(2),
        "payback_period": period,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_level_series_pays_back_in_one_division() {
        let answer = calculate(&json!({
            "initial_investment": "1000", "annual_cash_flows": ["250", "250", "250", "250"]
        }))
        .expect("calculated");
        assert_eq!(answer["equal_cash_flows"], json!(true));
        assert_eq!(answer["payback_period"], json!("4.00"));
        assert_eq!(answer["payback_years"], json!("4 years and 0.0 months"));
    }

    #[test]
    fn a_series_that_moves_pays_back_part_way_through_a_year() {
        // 1000 in, 400 then 800: the second year covers it after 600 of its 800, so 1.75 years
        let answer = calculate(&json!({
            "initial_investment": "1000", "annual_cash_flows": ["400", "800"]
        }))
        .expect("calculated");
        assert_eq!(answer["equal_cash_flows"], json!(false));
        assert_eq!(answer["payback_period"], json!("1.75"));
        assert_eq!(answer["payback_achieved_in_year"], json!(2));
        assert_eq!(answer["payback_years"], json!("1 years and 9.0 months"));
    }

    #[test]
    fn a_project_that_never_pays_back_says_so_rather_than_giving_a_figure() {
        let answer = calculate(&json!({
            "initial_investment": "1000", "annual_cash_flows": ["100", "200"]
        }))
        .expect("calculated");
        assert_eq!(answer["payback_period"], Value::Null);
        assert_eq!(
            answer["message"],
            json!("Payback not achieved within given cash flow period")
        );
    }

    #[test]
    fn a_level_series_that_pays_back_nothing_is_refused() {
        assert!(calculate(&json!({
            "initial_investment": "1000", "annual_cash_flows": ["0", "0"]
        }))
        .is_err());
    }

    #[test]
    fn comparison_ranks_the_quickest_first_and_leaves_out_the_hopeless() {
        let answer = compare(&json!({"projects": [
            {"name": "Slow", "initial_investment": 1000, "cash_flows": [250, 250, 250, 250]},
            {"name": "Quick", "initial_investment": 1000, "cash_flows": [500, 500]},
            {"name": "Hopeless", "initial_investment": 1000, "cash_flows": [100, 200]},
        ]}))
        .expect("compared");
        assert_eq!(answer["recommended_project"], json!("Quick"));
        assert_eq!(answer["project_comparison"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn the_average_flow_route_spreads_the_total_over_the_years() {
        let answer = average_cash_flow(&json!({
            "initial_investment": "1000", "total_cash_flows": "800", "years": 4
        }))
        .expect("averaged");
        assert_eq!(answer["average_annual_cash_flow"], json!("200.00"));
        assert_eq!(answer["payback_period"], json!("5.00"));
    }

    #[test]
    fn a_period_of_no_years_is_refused() {
        assert!(average_cash_flow(&json!({
            "initial_investment": "1000", "total_cash_flows": "800", "years": 0
        }))
        .is_err());
    }
}
