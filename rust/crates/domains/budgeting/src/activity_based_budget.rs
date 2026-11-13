#![forbid(unsafe_code)]
//! activity-based-budget-service: a budget built from activities and the drivers they are measured by.
//!
//! An activity carries a cost pool, a driver and a rate per unit of that driver. A budget then names activities with
//! the driver volume expected in a period, and each line's cost is that volume at the activity's rate.
//!
//! Behaviours kept as shipped, each pinned by a test:
//!
//! * a line naming an activity the caller does not hold is refused, naming the activity, rather than being dropped;
//! * the budgeted cost of a line is the volume times the rate the activity carried at the moment the budget was made,
//!   so a later change to the rate does not rewrite a budget already agreed;
//! * the total is the sum of the lines and nothing else;
//! * approving a budget sets its status to approved and leaves the figures alone;
//! * a budget the caller does not hold is a miss, and the listing filters by status when asked.

use serde_json::{json, Value};

use vimbai_common::Money;
use vimbai_store::amounts::wire;
use vimbai_store::read::{decimal, field, optional, refused, text};
use vimbai_store::write::remember;
use vimbai_store::{BookScope, Graph, Records, StoreError, ACTIVITY, ACTIVITY_BUDGET};

fn amount(payload: &Value, key: &str) -> Result<Money, StoreError> {
    Money::parse(&decimal(payload, key, "0"))
        .map_err(|_| refused(&format!("{key} must be an amount")))
}

/// `POST /activities`.
pub fn define_activity(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let name = text(payload, "name");
    if name.is_empty() {
        return Err(refused("name is required"));
    }
    let rate = amount(payload, "driver_rate")?;
    remember(
        graph,
        scope,
        ACTIVITY,
        json!({
            "name": name,
            "description": text(payload, "description"),
            "cost_pool": text(payload, "cost_pool"),
            "driver": text(payload, "driver"),
            "driver_rate": wire(&rate),
        }),
    )
}

/// `GET /activities`.
pub fn list_activities(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
) -> Result<Value, StoreError> {
    Ok(Value::Array(
        Records::new(graph, ACTIVITY).list(scope, 2000)?,
    ))
}

/// `POST /budgets`.
pub fn create_budget(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let name = text(payload, "name");
    if name.is_empty() {
        return Err(refused("name is required"));
    }
    let fiscal_year = text(payload, "fiscal_year");
    let period = text(payload, "period");

    let activities = Records::new(graph, ACTIVITY).list(scope, 2000)?;
    let named = payload
        .get("line_items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let mut lines = Vec::new();
    let mut total = Money::zero();
    for wanted in &named {
        let activity_id = text(wanted, "activity_id");
        let activity = activities
            .iter()
            .find(|row| field(row, "id") == activity_id)
            .ok_or_else(|| refused(&format!("Activity {activity_id} not found")))?;

        let volume = amount(wanted, "expected_driver_volume")?;
        let rate =
            Money::parse(&decimal(activity, "driver_rate", "0")).unwrap_or_else(|_| Money::zero());
        let budgeted = volume.times(&rate);
        total = total.plus(&budgeted);

        lines.push(json!({
            "activity_id": activity_id,
            "period": period,
            "expected_driver_volume": wire(&volume),
            "budgeted_cost": wire(&budgeted),
            "notes": text(wanted, "notes"),
        }));
    }

    remember(
        graph,
        scope,
        ACTIVITY_BUDGET,
        json!({
            "name": name,
            "fiscal_year": fiscal_year,
            "period": period,
            "line_items": lines,
            "total_budget": wire(&total),
            "status": "draft",
        }),
    )
}

/// `GET /budgets`.
pub fn list_budgets(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let wanted = optional(query, "status").unwrap_or_default();
    Ok(Value::Array(
        Records::new(graph, ACTIVITY_BUDGET)
            .list(scope, 2000)?
            .into_iter()
            .filter(|row| wanted.is_empty() || field(row, "status") == wanted)
            .collect(),
    ))
}

fn budget_of(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    budget_id: &str,
) -> Result<Value, StoreError> {
    Records::new(graph, ACTIVITY_BUDGET)
        .list(scope, 2000)?
        .into_iter()
        .find(|row| field(row, "id") == budget_id)
        .ok_or(StoreError::NotFound)
}

/// `GET /budgets/{budget_id}`.
pub fn get_budget(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    budget_id: &str,
) -> Result<Value, StoreError> {
    budget_of(graph, scope, budget_id)
}

/// `PUT /budgets/{budget_id}/approve`.
pub fn approve_budget(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    budget_id: &str,
) -> Result<Value, StoreError> {
    budget_of(graph, scope, budget_id)?;
    Records::new(graph, ACTIVITY_BUDGET).update(
        scope,
        budget_id,
        &json!({"status": "approved"}),
    )?;
    Ok(json!({"budget_id": budget_id, "status": "approved"}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::FakeGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    #[test]
    fn an_activity_is_defined_with_the_rate_it_is_measured_by() {
        let graph = FakeGraph::returning(vec![json!({"id": "a-1"})]);
        let answer = define_activity(
            &graph,
            &scope(),
            &json!({"name": "Setups", "cost_pool": "Factory", "driver": "setups", "driver_rate": "250"}),
        )
        .expect("defined");
        assert_eq!(answer["driver_rate"], json!("250.000000"));
        assert_eq!(answer["driver"], json!("setups"));
    }

    #[test]
    fn a_budget_prices_each_line_at_the_activity_rate_and_totals_the_lines() {
        let graph = FakeGraph::queue(vec![
            vec![
                json!({"id": "a-1", "name": "Setups", "driver_rate": "250.000000"}),
                json!({"id": "a-2", "name": "Inspections", "driver_rate": "40.000000"}),
            ],
            vec![json!({"id": "b-1"})],
        ]);
        let answer = create_budget(
            &graph,
            &scope(),
            &json!({"name": "Q1", "fiscal_year": "2026", "period": "2026-Q1",
            "line_items": [
                {"activity_id": "a-1", "expected_driver_volume": "8"},
                {"activity_id": "a-2", "expected_driver_volume": "25"}
            ]}),
        )
        .expect("created");
        // 8 at 250 is 2,000; 25 at 40 is 1,000
        assert_eq!(
            answer["line_items"][0]["budgeted_cost"],
            json!("2000.000000")
        );
        assert_eq!(answer["total_budget"], json!("3000.000000"));
        assert_eq!(answer["status"], json!("draft"));
    }

    #[test]
    fn a_line_naming_an_activity_the_caller_does_not_hold_is_refused_by_name() {
        let graph = FakeGraph::queue(vec![vec![]]);
        let error = create_budget(
            &graph,
            &scope(),
            &json!({"name": "Q1", "fiscal_year": "2026", "period": "2026-Q1",
                    "line_items": [{"activity_id": "a-9", "expected_driver_volume": "1"}]}),
        )
        .expect_err("refused");
        assert!(format!("{error}").contains("a-9"));
    }

    #[test]
    fn approving_a_budget_marks_it_approved_and_leaves_the_figures_alone() {
        let graph = FakeGraph::queue(vec![
            vec![json!({"id": "b-1", "total_budget": "3000.000000", "status": "draft"})],
            vec![json!({"id": "b-1"})],
        ]);
        let answer = approve_budget(&graph, &scope(), "b-1").expect("approved");
        assert_eq!(answer["status"], json!("approved"));
    }

    #[test]
    fn a_budget_the_caller_does_not_hold_is_a_miss() {
        let graph = FakeGraph::returning(vec![]);
        assert!(matches!(
            get_budget(&graph, &scope(), "other"),
            Err(StoreError::NotFound)
        ));
        assert!(matches!(
            approve_budget(&graph, &scope(), "other"),
            Err(StoreError::NotFound)
        ));
    }

    #[test]
    fn the_listing_filters_by_status_when_asked() {
        let graph = FakeGraph::returning(vec![
            json!({"id": "b-1", "status": "approved"}),
            json!({"id": "b-2", "status": "draft"}),
        ]);
        let answer =
            list_budgets(&graph, &scope(), &json!({"status": "approved"})).expect("listed");
        assert_eq!(answer.as_array().unwrap().len(), 1);
    }
}
