//! Target costing, ported from target-costing-service.
//!
//! Every money figure is a decimal, not a float: a selling price of 10^24 keeps all
//! of its digits through the subtraction, and a component split adds back to the
//! total. Percentages are carried to two places the way the service rounded them.
//!
//! Money leaves this module as a decimal string so the wire cannot round it. The
//! Python service in front of it still publishes floats; the seam converts there,
//! not here.

use serde_json::{json, Value};
use vimbai_common::Money;

fn money_of(value: &Value) -> Result<Money, String> {
    match value {
        Value::String(text) => Money::parse(text).map_err(|e| e.0),
        Value::Number(number) => Money::from_f64_shortest(number.as_f64().unwrap_or(0.0))
            .map_err(|e| format!("{}: {e}", number)),
        Value::Null => Ok(Money::zero()),
        other => Err(format!("expected an amount, found {other}")),
    }
}

fn text_of<'a>(payload: &'a Value, key: &str) -> &'a str {
    payload.get(key).and_then(Value::as_str).unwrap_or("")
}

/// Components arrive either as an object of name to amount or as `[name, amount]`
/// pairs. The pairs form carries the caller's order explicitly, which is what the
/// service relied on when it built the list from a dict.
fn components_of(payload: &Value) -> Result<Vec<(String, Money)>, String> {
    let Some(components) = payload.get("component_costs") else {
        return Ok(Vec::new());
    };

    match components {
        Value::Object(map) => {
            let mut out = Vec::with_capacity(map.len());
            for (name, amount) in map {
                out.push((name.clone(), money_of(amount)?));
            }
            Ok(out)
        }
        Value::Array(pairs) => {
            let mut out = Vec::with_capacity(pairs.len());
            for pair in pairs {
                let items = pair
                    .as_array()
                    .ok_or_else(|| "a component pair must be [name, amount]".to_string())?;
                let name = items
                    .first()
                    .and_then(Value::as_str)
                    .ok_or_else(|| "a component pair needs a name".to_string())?;
                let amount = items
                    .get(1)
                    .ok_or_else(|| "a component pair needs an amount".to_string())?;
                out.push((name.to_string(), money_of(amount)?));
            }
            Ok(out)
        }
        Value::Null => Ok(Vec::new()),
        other => Err(format!(
            "component_costs must be an object or pairs, found {other}"
        )),
    }
}

/// Target costing for one product.
///
/// The acceptance rule is the service's: a reduction is feasible while it stays
/// within 30% of current cost.
pub fn evaluate(payload: &Value) -> Result<Value, String> {
    let selling = money_of(payload.get("target_selling_price").unwrap_or(&Value::Null))?;
    let margin_pct = money_of(
        payload
            .get("desired_profit_margin_pct")
            .unwrap_or(&Value::Null),
    )?;
    let current = money_of(payload.get("current_cost").unwrap_or(&Value::Null))?;

    // target_profit = price x margin/100, exact before the two-place report rounding
    let hundred = Money::from_i64(100);
    let target_profit = selling.times(&margin_pct.over(&hundred)).round(2);
    let target_cost = selling.minus(&target_profit).round(2);
    let reduction = current.minus(&target_cost);
    let reduction_pct = reduction.ratio_pct(&current, 2);

    let ceiling = current.times(&Money::from_i64(30)).over(&hundred).round(2);
    let feasible = reduction <= ceiling;

    let component_amounts = components_of(payload)?;
    let total: Money = Money::sum(component_amounts.iter().map(|(_, amount)| amount));

    let mut analysis = Vec::with_capacity(component_amounts.len());
    for (name, amount) in &component_amounts {
        let proportion = amount.ratio_pct(&total, 1);
        let suggested = amount.times(&reduction_pct).over(&hundred).round(2);
        analysis.push(json!({
            "component": name,
            "current_cost": amount.to_fixed(2),
            "proportion_of_total": proportion.to_fixed(1),
            "suggested_reduction": suggested.to_fixed(2),
            "target_cost": amount.minus(&suggested).round(2).to_fixed(2),
        }));
    }

    Ok(json!({
        "company_id": text_of(payload, "company_id"),
        "product_name": text_of(payload, "product_name"),
        "target_selling_price": selling.round(2).to_fixed(2),
        "desired_margin": margin_pct.round(2).to_fixed(2),
        "target_profit": target_profit.to_fixed(2),
        "target_cost": target_cost.to_fixed(2),
        "current_cost": current.round(2).to_fixed(2),
        "cost_reduction_needed": reduction.round(2).to_fixed(2),
        "cost_reduction_pct": reduction_pct.to_fixed(2),
        "feasible": feasible,
        "component_analysis": analysis,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn payload(selling: &str, current: &str) -> Value {
        json!({
            "company_id": "c1",
            "product_name": "Widget",
            "target_selling_price": selling,
            "desired_profit_margin_pct": "20",
            "current_cost": current,
            "component_costs": {"parts": "60", "labour": "40"},
        })
    }

    #[test]
    fn a_target_cost_is_the_price_less_the_margin() {
        let out = evaluate(&payload("100", "120")).unwrap();
        assert_eq!(out["target_profit"], "20.00");
        assert_eq!(out["target_cost"], "80.00");
        assert_eq!(out["cost_reduction_needed"], "40.00");
        assert_eq!(out["cost_reduction_pct"], "33.33");
        // 40 is more than 30% of 120, so the target is out of reach
        assert_eq!(out["feasible"], false);
    }

    #[test]
    fn components_keep_their_share_of_a_large_total() {
        let out = evaluate(&json!({
            "company_id": "c1",
            "product_name": "Widget",
            "target_selling_price": "1000000000000000000000000",
            "desired_profit_margin_pct": "10",
            "current_cost": "900000000000000000000000",
            "component_costs": {"parts": "1", "labour": "2"},
        }))
        .unwrap();
        // 10^24 less a tenth of it, with every digit intact
        assert_eq!(out["target_profit"], "100000000000000000000000.00");
        assert_eq!(out["target_cost"], "900000000000000000000000.00");
        assert_eq!(out["cost_reduction_needed"], "0.00");
    }

    #[test]
    fn a_component_split_adds_back_to_the_whole() {
        let out = evaluate(&payload("100", "120")).unwrap();
        let parts: Vec<&str> = out["component_analysis"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["current_cost"].as_str().unwrap())
            .collect();
        assert_eq!(parts, vec!["60.00", "40.00"]);
        assert_eq!(out["component_analysis"][0]["proportion_of_total"], "60.0");
    }

    #[test]
    fn a_zero_current_cost_does_not_divide() {
        let out = evaluate(&payload("100", "0")).unwrap();
        assert_eq!(out["cost_reduction_pct"], "0.00");
    }
}

#[cfg(test)]
mod pair_tests {
    use super::*;

    #[test]
    fn component_pairs_keep_their_order() {
        let out = evaluate(&json!({
            "company_id": "c1",
            "product_name": "Widget",
            "target_selling_price": "100",
            "desired_profit_margin_pct": "20",
            "current_cost": "120",
            "component_costs": [["materials", "60"], ["labour", "40"]],
        }))
        .unwrap();
        let names: Vec<&str> = out["component_analysis"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["component"].as_str().unwrap())
            .collect();
        assert_eq!(names, vec!["materials", "labour"]);
    }
}
