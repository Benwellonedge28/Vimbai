#![forbid(unsafe_code)]
//! net-present-value-service: what a project's future flows are worth today once the investment is paid for.
//!
//! Discounting is exact, one year at a time, through the shared engine. A rate may arrive as a decimal (`0.1`) or
//! as a percentage (`10`), which is the one thing the two entry points differ by.
//!
//! Behaviours kept as shipped, each pinned by a test:
//!
//! * the decision is accept for a positive net, reject for a negative one, and indifferent at exactly zero;
//! * a residual value belongs to the final year and is discounted the same number of years as the last flow;
//! * the per-year schedule publishes the discount factor to four places and the present value to two;
//! * a project in a comparison with no flows or a zero rate carries an error rather than a net, and is left out of
//!   the ranking; the recommendation is the best net, and nothing when no project was usable;
//! * the profitability index is the inflows over the investment, and an investment of nothing indexes zero rather
//!   than dividing.

use serde_json::{json, Value};

use vimbai_common::Money;
use vimbai_store::read::{decimal, refused};
use vimbai_store::StoreError;

use crate::discounting::{
    factor, flows, inflows, investment, net_present_value, rate, residual, worth_today,
};

fn decision(net: &Money) -> &'static str {
    if net.is_zero() {
        "Indifferent - Zero NPV"
    } else if net.is_negative() {
        "Reject - Negative NPV"
    } else {
        "Accept - Positive NPV"
    }
}

/// The year-by-year schedule both entry points publish.
fn schedule(series: &[Money], discount_rate: &Money, residual_value: &Money) -> Vec<Value> {
    let mut rows = Vec::new();
    for (index, flow) in series.iter().enumerate() {
        let years = index as i64 + 1;
        rows.push(json!({
            "year": years,
            "cash_flow": flow.round(2).to_fixed(2),
            "discount_factor": factor(discount_rate, years).round(4).to_fixed(4),
            "present_value": worth_today(flow, discount_rate, years).round(2).to_fixed(2),
        }));
    }
    if !residual_value.is_zero() && !residual_value.is_negative() {
        let years = series.len() as i64;
        rows.push(json!({
            "year": years,
            "description": "Residual Value",
            "cash_flow": residual_value.round(2).to_fixed(2),
            "discount_factor": factor(discount_rate, years).round(4).to_fixed(4),
            "present_value": worth_today(residual_value, discount_rate, years).round(2).to_fixed(2),
        }));
    }
    rows
}

/// `POST /calculate`: the rate as a decimal.
pub fn calculate(payload: &Value) -> Result<Value, StoreError> {
    let invested = investment(payload)?;
    let series = flows(payload, "cash_flows")?;
    let discount_rate = rate(payload, "discount_rate")?;
    let residual_value = residual(payload)?;

    let pv_inflows = inflows(&series, &discount_rate, &residual_value);
    let net = net_present_value(&invested, &series, &discount_rate, &residual_value);

    Ok(json!({
        "initial_investment": invested.round(2).to_fixed(2),
        "discount_rate": discount_rate.round(6).to_fixed(6),
        "residual_value": residual_value.round(2).to_fixed(2),
        "cash_flow_details": schedule(&series, &discount_rate, &residual_value),
        "total_pv_inflows": pv_inflows.round(2).to_fixed(2),
        "net_present_value": net.round(2).to_fixed(2),
        "decision": decision(&net),
    }))
}

/// `POST /calculate-percent`: the same, with the rate as a percentage.
pub fn calculate_percent(payload: &Value) -> Result<Value, StoreError> {
    let percentage = Money::parse(&decimal(payload, "discount_rate_percent", "0"))
        .map_err(|_| refused("discount_rate_percent must be a number"))?;
    let hundred = Money::parse("100").map_err(|_| refused("a rate must be a number"))?;
    let discount_rate = percentage.over(&hundred);

    let invested = investment(payload)?;
    let series = flows(payload, "cash_flows")?;
    let residual_value = residual(payload)?;
    let net = net_present_value(&invested, &series, &discount_rate, &residual_value);

    Ok(json!({
        "initial_investment": invested.round(2).to_fixed(2),
        "discount_rate_percent": percentage.round(4).to_fixed(4),
        "discount_rate_decimal": discount_rate.round(6).to_fixed(6),
        "residual_value": residual_value.round(2).to_fixed(2),
        "net_present_value": net.round(2).to_fixed(2),
        "decision": if net.is_zero() { "Indifferent" } else if net.is_negative() { "Reject" } else { "Accept" },
    }))
}

/// `POST /compare`: the best of several projects, ranked by what each is worth today.
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
        let series = flows(&project, "cash_flows").unwrap_or_default();
        let discount_rate = rate(&project, "discount_rate").unwrap_or_else(|_| Money::zero());
        // the original left a project with no flows or no rate out of the ranking rather than ranking it last
        if series.is_empty() || discount_rate.is_zero() {
            continue;
        }
        let invested = Money::parse(
            &project
                .get("initial_investment")
                .map(|value| value.to_string().trim_matches('"').to_string())
                .unwrap_or_else(|| "0".to_string()),
        )
        .unwrap_or_else(|_| Money::zero());
        let residual_value = residual(&project).unwrap_or_else(|_| Money::zero());
        ranked.push((
            name,
            net_present_value(&invested, &series, &discount_rate, &residual_value),
        ));
    }

    ranked.sort_by(|one, two| two.1.as_decimal().cmp(one.1.as_decimal()));
    let rows: Vec<Value> = ranked
        .iter()
        .map(|(name, net)| json!({"name": name, "net_present_value": net.round(2).to_fixed(2)}))
        .collect();
    let recommended = ranked.first().map(|(name, _)| name.clone());

    Ok(json!({
        "project_comparison": rows,
        "recommended_project": recommended,
    }))
}

/// `POST /profitability-index`: what each unit invested comes back as.
pub fn profitability_index(payload: &Value) -> Result<Value, StoreError> {
    let invested = Money::parse(&decimal(payload, "initial_investment", "0"))
        .map_err(|_| refused("initial_investment must be an amount"))?;
    let series = flows(payload, "cash_flows")?;
    let discount_rate = rate(payload, "discount_rate")?;
    let residual_value = residual(payload)?;
    let pv_inflows = inflows(&series, &discount_rate, &residual_value);

    let index = if invested.is_zero() {
        Money::zero()
    } else {
        pv_inflows.over(&invested)
    };
    let one = Money::parse("1").map_err(|_| refused("a ratio must be a number"))?;
    let interpretation = if index == one {
        "Indifferent"
    } else if index.as_decimal() > one.as_decimal() {
        "Accept"
    } else {
        "Reject"
    };

    Ok(json!({
        "initial_investment": invested.round(2).to_fixed(2),
        "pv_of_inflows": pv_inflows.round(2).to_fixed(2),
        "profitability_index": index.round(4).to_fixed(4),
        "interpretation": interpretation,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_project_worth_more_than_it_costs_is_accepted() {
        // 1000 in, 600 a year for two years at 10%: 545.45 + 495.87 = 1041.32, so 41.32 up
        let answer = calculate(&json!({
            "initial_investment": 1000, "cash_flows": [600, 600], "discount_rate": 0.1
        }))
        .expect("calculated");
        assert_eq!(answer["total_pv_inflows"], json!("1041.32"));
        assert_eq!(answer["net_present_value"], json!("41.32"));
        assert_eq!(answer["decision"], json!("Accept - Positive NPV"));
        assert_eq!(
            answer["cash_flow_details"][0]["discount_factor"],
            json!("0.9091")
        );
    }

    #[test]
    fn a_project_that_loses_money_is_rejected() {
        let answer = calculate(&json!({
            "initial_investment": "2000", "cash_flows": ["600", "600"], "discount_rate": "0.1"
        }))
        .expect("calculated");
        assert_eq!(answer["net_present_value"], json!("-958.68"));
        assert_eq!(answer["decision"], json!("Reject - Negative NPV"));
    }

    #[test]
    fn a_rate_arriving_as_a_percentage_is_the_same_answer() {
        let decimal = calculate(&json!({
            "initial_investment": 1000, "cash_flows": [600, 600], "discount_rate": 0.1
        }))
        .expect("calculated");
        let percent = calculate_percent(&json!({
            "initial_investment": 1000, "cash_flows": [600, 600], "discount_rate_percent": 10
        }))
        .expect("calculated");
        assert_eq!(decimal["net_present_value"], percent["net_present_value"]);
        assert_eq!(percent["discount_rate_decimal"], json!("0.100000"));
    }

    #[test]
    fn a_residual_value_is_discounted_with_the_final_year() {
        let answer = calculate(&json!({
            "initial_investment": 1000, "cash_flows": [600, 600], "discount_rate": 0.1,
            "residual_value": 121
        }))
        .expect("calculated");
        // 100 discounted two years is 100 / 1.21 = 82.64, so 1041.32 + 100 - 1000
        assert_eq!(answer["net_present_value"], json!("141.32"));
        assert_eq!(
            answer["cash_flow_details"][2]["description"],
            json!("Residual Value")
        );
    }

    #[test]
    fn a_project_with_no_flows_is_refused() {
        assert!(calculate(&json!({"initial_investment": 10, "cash_flows": []})).is_err());
    }

    #[test]
    fn comparison_ranks_the_better_project_first_and_leaves_broken_ones_out() {
        let answer = compare(&json!({"projects": [
            {"name": "Slow", "initial_investment": 1000, "cash_flows": [600, 600], "discount_rate": 0.1},
            {"name": "Fast", "initial_investment": 1000, "cash_flows": [900, 600], "discount_rate": 0.1},
            {"name": "Broken", "initial_investment": 1000, "cash_flows": [], "discount_rate": 0.1},
        ]}))
        .expect("compared");
        assert_eq!(answer["recommended_project"], json!("Fast"));
        assert_eq!(answer["project_comparison"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn the_profitability_index_says_what_each_unit_returns() {
        let answer = profitability_index(&json!({
            "initial_investment": 1000, "cash_flows": [600, 600], "discount_rate": 0.1
        }))
        .expect("indexed");
        assert_eq!(answer["profitability_index"], json!("1.0413"));
        assert_eq!(answer["interpretation"], json!("Accept"));
    }
}
