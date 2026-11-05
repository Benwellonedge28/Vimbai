//! Deal financing structure, as the service proposed it.
//!
//! Three things about this are worth naming before anyone reads the figures as advice:
//!
//! * the tranche split is fixed at 60/25/15 of debt capacity with margins of zero, three and six
//!   points over the base rate, regardless of the deal;
//! * `financing_options` is not three comparable options. The first is the low-leverage case, the
//!   second is the *chosen* structure restated with percentages, and the third is the high-leverage
//!   case - and their `wacc` fields are in different units, a fraction on the first and third and a
//!   percentage on the second;
//! * `recommendations` is empty whenever the chosen structure is conservative, because all three
//!   checks are for excess rather than shortfall.
//!
//! All of it is kept as it was. A caller renders these fields; changing the units or the order
//! would change what a screen shows.

use serde_json::{Map, Value};

fn round2(value: f64) -> f64 {
    vimbai_common::numbers::round_dp(value, 2)
}

fn round4(value: f64) -> f64 {
    vimbai_common::numbers::round_dp(value, 4)
}

fn number(payload: &Value, key: &str) -> f64 {
    payload.get(key).and_then(Value::as_f64).unwrap_or(0.0)
}

/// `POST /structure` of the acquisition financing service.
pub fn structure(payload: &Value) -> Result<Value, String> {
    let deal_value = number(payload, "deal_value");
    let debt_capacity = number(payload, "acquirer_debt_capacity")
        .min(number(payload, "target_leverage") * deal_value);
    let equity_required = deal_value - debt_capacity;
    let equity_percentage = if deal_value == 0.0 {
        0.0
    } else {
        equity_required / deal_value
    };
    let leverage = if deal_value == 0.0 {
        0.0
    } else {
        debt_capacity / deal_value
    };

    let interest_rate = number(payload, "interest_rate");
    let equity_cost = number(payload, "equity_cost");
    let interest_expense = debt_capacity * interest_rate;
    let after_tax_cost = interest_expense * (1.0 - 0.25);
    let wacc = leverage * interest_rate * 0.75 + (1.0 - leverage) * equity_cost;

    let tranches = || {
        serde_json::json!([
            {
                "type": "Senior Debt",
                "amount": debt_capacity * 0.6,
                "rate": interest_rate,
                "term": 5,
                "percentage": 60,
            },
            {
                "type": "Mezzanine",
                "amount": debt_capacity * 0.25,
                "rate": interest_rate + 0.03,
                "term": 7,
                "percentage": 25,
            },
            {
                "type": "Equity",
                "amount": equity_required,
                "rate": equity_cost,
                "term": 0,
                "percentage": 15,
            },
        ])
    };

    let coverage = |share: f64| {
        if interest_rate == 0.0 {
            0.0
        } else {
            deal_value * 0.2 / (deal_value * share * interest_rate)
        }
    };

    let low = serde_json::json!({
        "debt_percentage": 40,
        "debt": deal_value * 0.4,
        "equity": deal_value * 0.6,
        "wacc": 0.4 * interest_rate * 0.75 + 0.6 * equity_cost,
        "interest_coverage": coverage(0.4),
    });
    let chosen = serde_json::json!({
        "debt": debt_capacity,
        "equity": equity_required,
        "debt_percentage": round2(leverage * 100.0),
        "equity_percentage": round2(equity_percentage * 100.0),
        "wacc": round2(wacc * 100.0),
    });
    let high = serde_json::json!({
        "debt_percentage": 70,
        "debt": deal_value * 0.7,
        "equity": deal_value * 0.3,
        "wacc": 0.7 * interest_rate * 0.75 + 0.3 * equity_cost,
        "interest_coverage": coverage(0.7),
    });

    let mut recommendations = Vec::new();
    if equity_percentage > 0.6 {
        recommendations.push(Value::String(
            "High equity contribution - consider taking more debt to optimize returns".into(),
        ));
    }
    if wacc > equity_cost {
        recommendations.push(Value::String(
            "Debt financing reduces WACC - consider higher leverage".into(),
        ));
    }
    if interest_expense > deal_value * 0.15 {
        recommendations.push(Value::String(
            "Interest burden is significant - model stress scenarios".into(),
        ));
    }

    let mut out = Map::new();
    out.insert(
        "deal_id".into(),
        payload.get("deal_id").cloned().unwrap_or(Value::Null),
    );
    out.insert(
        "optimal_structure".into(),
        serde_json::json!({
            "tranches": tranches(),
            "total_debt": debt_capacity,
            "total_equity": equity_required,
        }),
    );
    out.insert(
        "financing_options".into(),
        Value::Array(vec![low, chosen, high]),
    );
    out.insert(
        "leverage_analysis".into(),
        serde_json::json!({
            "total_leverage": round4(leverage),
            "interest_expense": interest_expense,
            "after_tax_cost": after_tax_cost,
        }),
    );
    out.insert("cost_of_capital".into(), Value::from(round2(wacc * 100.0)));
    out.insert("recommendations".into(), Value::Array(recommendations));
    Ok(Value::Object(out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn deal() -> Value {
        json!({
            "deal_id": "d-1", "deal_value": 10_000_000.0,
            "acquirer_debt_capacity": 4_000_000.0, "target_leverage": 0.5,
            "interest_rate": 0.08, "equity_cost": 0.12
        })
    }

    #[test]
    fn debt_is_capped_by_both_the_appetite_and_the_leverage_target() {
        let out = structure(&deal()).unwrap();
        assert_eq!(out["optimal_structure"]["total_debt"], json!(4_000_000.0));
        assert_eq!(out["optimal_structure"]["total_equity"], json!(6_000_000.0));
    }

    #[test]
    fn the_middle_option_restates_the_chosen_structure_in_percentages() {
        let out = structure(&deal()).unwrap();
        let options = out["financing_options"].as_array().unwrap();
        assert_eq!(options.len(), 3);
        assert_eq!(options[1]["debt_percentage"], json!(40.0));
        assert_eq!(options[1]["equity_percentage"], json!(60.0));
        assert_eq!(options[1]["wacc"], json!(9.6));
    }

    #[test]
    fn a_zero_deal_value_does_not_divide_by_nothing() {
        let out = structure(&json!({"deal_id": "d-2", "deal_value": 0.0})).unwrap();
        assert_eq!(out["leverage_analysis"]["total_leverage"], json!(0.0));
        assert_eq!(out["cost_of_capital"], json!(0.0));
    }

    #[test]
    fn a_conservative_structure_earns_no_recommendations() {
        let out = structure(&deal()).unwrap();
        assert_eq!(out["recommendations"], json!([]));
    }

    #[test]
    fn an_equity_heavy_deal_asks_for_more_debt() {
        let out = structure(&json!({
            "deal_id": "d-3", "deal_value": 1_000_000.0,
            "acquirer_debt_capacity": 100_000.0, "target_leverage": 0.2,
            "interest_rate": 0.2, "equity_cost": 0.1
        }))
        .unwrap();
        let notes = out["recommendations"].as_array().unwrap();
        assert!(notes
            .iter()
            .any(|n| n.as_str().unwrap().contains("High equity contribution")));
    }
}
