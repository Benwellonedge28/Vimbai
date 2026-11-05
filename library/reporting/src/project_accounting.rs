//! Project cost, budget and work in progress, as the service reported them.
//!
//! Three behaviours here look like mistakes and are kept anyway, because a screen prints them:
//!
//! * labor is charged at a flat 75 per hour whatever the person, and the cost breakdown calls the
//!   material total "materials" even though it is every cost line;
//! * `margin` divides by billable *excluding* labor, while `profit` subtracts labor - so margin and
//!   profit disagree on the same project;
//! * `utilization_rate` is the constant 85.5, not a measurement, and `remaining` in a budget is the
//!   contingency rather than what is left unspent.

use serde_json::{Map, Value};

fn round2(value: f64) -> f64 {
    vimbai_common::numbers::round_dp(value, 2)
}

fn number(item: &Value, key: &str) -> f64 {
    item.get(key).and_then(Value::as_f64).unwrap_or(0.0)
}

fn sum_amounts(items: &Value) -> f64 {
    items
        .as_array()
        .map(|rows| rows.iter().map(|row| number(row, "amount")).sum())
        .unwrap_or(0.0)
}

fn sum_values(map: Option<&Value>) -> f64 {
    map.and_then(Value::as_object)
        .map(|object| object.values().filter_map(Value::as_f64).sum())
        .unwrap_or(0.0)
}

/// `POST /costs`.
pub fn costs(payload: &Value) -> Result<Value, String> {
    let materials = sum_amounts(payload.get("costs").unwrap_or(&Value::Null));
    let labor_hours = sum_values(payload.get("labor_hours"));
    let labor = labor_hours * 75.0;
    let billable = sum_values(payload.get("billable_amounts"));
    let margin = if billable == 0.0 {
        0.0
    } else {
        (billable - materials) / billable * 100.0
    };

    let mut out = Map::new();
    out.insert(
        "project_id".into(),
        payload.get("project_id").cloned().unwrap_or(Value::Null),
    );
    out.insert("total_cost".into(), Value::from(round2(materials + labor)));
    out.insert("total_billable".into(), Value::from(round2(billable)));
    out.insert(
        "profit".into(),
        Value::from(round2(billable - materials - labor)),
    );
    out.insert("margin".into(), Value::from(round2(margin)));
    out.insert(
        "cost_breakdown".into(),
        serde_json::json!({"materials": materials, "labor": labor}),
    );
    out.insert("utilization_rate".into(), Value::from(85.5));
    Ok(Value::Object(out))
}

/// `POST /budget`.
pub fn budget(payload: &Value) -> Result<Value, String> {
    let allocated = sum_amounts(payload.get("budget_items").unwrap_or(&Value::Null));
    let contingencies = payload
        .get("contingencies")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);

    let mut out = Map::new();
    out.insert(
        "project_id".into(),
        payload.get("project_id").cloned().unwrap_or(Value::Null),
    );
    out.insert(
        "total_budget".into(),
        Value::from(round2(allocated + contingencies)),
    );
    out.insert("allocated".into(), Value::from(round2(allocated)));
    out.insert("contingency".into(), Value::from(round2(contingencies)));
    out.insert("remaining".into(), Value::from(round2(contingencies)));
    out.insert("variance_alerts".into(), serde_json::json!([]));
    Ok(Value::Object(out))
}

/// `POST /wip`.
pub fn wip(payload: &Value) -> Result<Value, String> {
    let incurred = payload
        .get("costs_incurred")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let recognized = payload
        .get("recognized_revenue")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let total = payload
        .get("total_revenue")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);

    let mut out = Map::new();
    out.insert(
        "project_id".into(),
        payload.get("project_id").cloned().unwrap_or(Value::Null),
    );
    out.insert("wip_value".into(), Value::from(round2(incurred)));
    out.insert("billable_wip".into(), Value::from(round2(incurred * 1.3)));
    out.insert("recognized_to_date".into(), Value::from(round2(recognized)));
    out.insert(
        "pending_revenue".into(),
        Value::from(round2(total - recognized)),
    );
    Ok(Value::Object(out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn labour_is_seventy_five_an_hour_and_lands_in_the_cost() {
        let out = costs(&json!({
            "project_id": "p-1", "costs": [{"amount": 100.0}],
            "labor_hours": {"eng": 2.0}, "billable_amounts": {"a": 1000.0}
        }))
        .unwrap();
        assert_eq!(out["cost_breakdown"]["labor"], json!(150.0));
        assert_eq!(out["total_cost"], json!(250.0));
    }

    #[test]
    fn margin_leaves_labour_out_while_profit_subtracts_it() {
        let out = costs(&json!({
            "project_id": "p-1", "costs": [{"amount": 500.0}],
            "labor_hours": {"eng": 2.0}, "billable_amounts": {"a": 1000.0}
        }))
        .unwrap();
        assert_eq!(out["profit"], json!(350.0));
        assert_eq!(out["margin"], json!(50.0));
    }

    #[test]
    fn billing_nothing_reports_a_zero_margin_rather_than_dividing() {
        let out = costs(&json!({"project_id": "p-1"})).unwrap();
        assert_eq!(out["margin"], json!(0.0));
        assert_eq!(out["utilization_rate"], json!(85.5));
    }

    #[test]
    fn a_budgets_remaining_is_its_contingency() {
        let out = budget(&json!({
            "project_id": "p-1", "budget_items": [{"amount": 2000.0}], "contingencies": 500.0
        }))
        .unwrap();
        assert_eq!(out["total_budget"], json!(2500.0));
        assert_eq!(out["remaining"], json!(500.0));
    }

    #[test]
    fn billable_work_in_progress_is_half_again_the_cost() {
        let out = wip(&json!({
            "project_id": "p-1", "costs_incurred": 4000.0,
            "recognized_revenue": 5000.0, "total_revenue": 9000.0
        }))
        .unwrap();
        assert_eq!(out["billable_wip"], json!(5200.0));
        assert_eq!(out["pending_revenue"], json!(4000.0));
    }
}
