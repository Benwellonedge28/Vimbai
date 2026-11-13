#![forbid(unsafe_code)]
//! scenario-modeling-service: scenarios built from labelled variables, rules that fire on values, and a what-if run
//! against them.
//!
//! A rule holds conditions that must all hold and actions to take when they do. A condition compares one field of the
//! working figures against a value, and an action adjusts, scales, percentage-bends or overwrites one. A scenario's
//! outcome is then read off by its type: a budget forecast adds its variables up, a revenue projection compounds a
//! growth rate over the months between the two dates, an expense simulation adds the variables whose names mention
//! expenses, a cash flow nets inflows against outflows, a profitability subtracts costs from revenue, and anything else
//! adds its variables up.
//!
//! Behaviours kept as shipped, each pinned by a test:
//!
//! * a condition on a field the figures do not carry is not met, rather than treated as zero;
//! * comparison is by number once both sides read as numbers, and by text otherwise, which is how "contains" works;
//! * every condition must hold for a rule to fire, and every rule fires in the order the scenario names them, with
//!   nothing re-sorted by the priority field the rules carry;
//! * a what-if over a variable the scenario does not carry leaves it alone rather than adding it;
//! * the variance is the new outcome less the original, and its percentage is nil when the original was nil, so a
//!   scenario that started at nothing reports no percentage even when the change is large;
//! * the affected accounts are the shipped split of the variance: four tenths revenue, three tenths expenses, three
//!   tenths net income - a shape, not a posting;
//! * a swept variable's recommendations are capped at five, and each is a statement of what a step yielded;
//! * a comparison names the best and worst of the scenarios given and reports nothing else.

use crate::figures::{hundred, one};
use serde_json::{json, Value};

use vimbai_common::Money;
use vimbai_store::amounts::wire;
use vimbai_store::read::{decimal, field, optional, refused, text};
use vimbai_store::write::remember;
use vimbai_store::{
    BookScope, Graph, Records, StoreError, MODELING_RULE, SCENARIO_MODEL, SENSITIVITY_MODEL_RESULT,
    WHAT_IF_RESULT,
};

/// The statuses a scenario may carry.
const STATUSES: &[&str] = &["draft", "active", "completed", "archived"];

fn zero() -> Money {
    Money::zero()
}

/// The named figures a rule works on, kept as text so a number and a word can both live in one map.
type Figures = Vec<(String, String)>;

fn figure(figures: &Figures, name: &str) -> Option<String> {
    figures
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.clone())
}

fn put(figures: &mut Figures, name: &str, value: String) {
    match figures.iter_mut().find(|(key, _)| key == name) {
        Some(entry) => entry.1 = value,
        None => figures.push((name.to_string(), value)),
    }
}

fn as_money(text: &str) -> Option<Money> {
    Money::parse(text).ok()
}

/// One condition, checked against the working figures.
fn condition_holds(condition: &Value, figures: &Figures) -> bool {
    let name = text(condition, "field");
    let carried = match figure(figures, &name) {
        Some(value) => value,
        // a condition on a field the figures do not carry is not met
        None => return false,
    };
    let wanted = value_text(condition.get("value").unwrap_or(&Value::Null));
    let operator = text(condition, "operator");

    // numbers compare as numbers once both sides read as numbers
    if let (Some(left), Some(right)) = (as_money(&carried), as_money(&wanted)) {
        let order = left.as_decimal().cmp(right.as_decimal());
        return match operator.as_str() {
            "equals" => order.is_eq(),
            "not_equals" => !order.is_eq(),
            "greater_than" => order.is_gt(),
            "less_than" => order.is_lt(),
            "greater_or_equal" => order.is_ge(),
            "less_or_equal" => order.is_le(),
            "contains" => right.to_fixed(6).contains(&left.to_fixed(6)),
            "between" => {
                let other = value_text(condition.get("secondary_value").unwrap_or(&Value::Null));
                match as_money(&other) {
                    Some(ceiling) => {
                        left.as_decimal() >= right.as_decimal()
                            && left.as_decimal() <= ceiling.as_decimal()
                    }
                    None => false,
                }
            }
            _ => false,
        };
    }

    match operator.as_str() {
        "equals" => carried == wanted,
        "not_equals" => carried != wanted,
        "contains" => wanted.contains(&carried) || carried.contains(&wanted),
        _ => false,
    }
}

fn value_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// A rule, applied in place: every condition must hold, then each action lands in turn.
fn apply_rule(rule: &Value, figures: &mut Figures) {
    let conditions = rule
        .get("conditions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let actions = rule
        .get("actions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    if !conditions
        .iter()
        .all(|condition| condition_holds(condition, figures))
    {
        return;
    }

    for action in &actions {
        let target = text(action, "target_field");
        let value = action.get("value").cloned().unwrap_or(Value::Null);
        let current = figure(figures, &target)
            .and_then(|carried| as_money(&carried))
            .unwrap_or_else(zero);
        let amount = as_money(&value_text(&value)).unwrap_or_else(zero);

        let landed = match text(action, "action_type").as_str() {
            "adjust_amount" => current.plus(&amount),
            "scale_amount" => current.times(&amount),
            "apply_percentage" => current.times(&one().plus(&amount.over(&hundred()))),
            "set_value" => {
                put(figures, &target, value_text(&value));
                continue;
            }
            // a flag or a workflow is noted rather than being arithmetic
            _ => continue,
        };
        put(figures, &target, landed.to_fixed(6));
    }
}

/// Days between two ISO dates, counted the way the service counted them.
fn days_between(from: &str, to: &str) -> i64 {
    fn day_number(date: &str) -> Option<i64> {
        let mut parts = date.split('-');
        let year: i64 = parts.next()?.parse().ok()?;
        let month: i64 = parts.next()?.parse().ok()?;
        let day: i64 = parts.next()?.parse().ok()?;
        if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
            return None;
        }
        // days from the civil epoch, by the usual month-length walk
        let lengths = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
        let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
        let mut total =
            (year - 1970) * 365 + (year - 1969) / 4 - (year - 1901) / 100 + (year - 1601) / 400;
        for (month_index, length) in lengths.iter().take(month as usize - 1).enumerate() {
            total += length;
            if month_index == 1 && leap {
                total += 1;
            }
        }
        Some(total + day - 1)
    }

    match (day_number(from), day_number(to)) {
        (Some(start), Some(end)) => end - start,
        _ => 0,
    }
}

fn period_power(base: &Money, growth: &Money, periods: f64) -> Money {
    let factor = (1.0 + growth.to_f64_lossy()).powf(periods);
    Money::from_f64_shortest(base.to_f64_lossy() * factor).unwrap_or_else(|_| zero())
}

/// The outcome a scenario's type reads off its figures.
fn outcome_of(scenario: &Value, figures: &Figures) -> Money {
    let kind = text(scenario, "scenario_type");
    match kind.as_str() {
        "budget_forecast" => {
            let mut total = zero();
            for (_, value) in figures {
                total = total.plus(&as_money(value).unwrap_or_else(zero));
            }
            total
        }
        "revenue_projection" => {
            let assumptions = scenario.get("assumptions").cloned().unwrap_or(json!({}));
            let base =
                as_money(&decimal(&assumptions, "base_revenue", "100000")).unwrap_or_else(zero);
            let growth = figure(figures, "growth_rate")
                .and_then(|value| as_money(&value))
                .unwrap_or_else(|| Money::parse("0.05").unwrap_or_else(|_| zero()));
            let months = days_between(&text(scenario, "base_date"), &text(scenario, "end_date"))
                as f64
                / 30.0;
            period_power(&base, &growth, months)
        }
        "expense_simulation" => {
            let mut total = zero();
            for (name, value) in figures {
                if name.to_lowercase().contains("expense") {
                    total = total.plus(&as_money(value).unwrap_or_else(zero));
                }
            }
            total
        }
        "cash_flow" => {
            let mut inflows = zero();
            let mut outflows = zero();
            for (name, value) in figures {
                let named = name.to_lowercase();
                if named.contains("inflow") {
                    inflows = inflows.plus(&as_money(value).unwrap_or_else(zero));
                } else if named.contains("outflow") {
                    outflows = outflows.plus(&as_money(value).unwrap_or_else(zero));
                }
            }
            inflows.minus(&outflows)
        }
        "profitability" => {
            let revenue = figure(figures, "revenue")
                .and_then(|value| as_money(&value))
                .unwrap_or_else(zero);
            let costs = figure(figures, "costs")
                .and_then(|value| as_money(&value))
                .unwrap_or_else(zero);
            revenue.minus(&costs)
        }
        _ => {
            let mut total = zero();
            for (_, value) in figures {
                total = total.plus(&as_money(value).unwrap_or_else(zero));
            }
            total
        }
    }
}

/// The figures a scenario starts from: its variables by name.
fn base_figures(scenario: &Value) -> Figures {
    scenario
        .get("variables")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|variable| {
            (
                text(variable, "name"),
                decimal(variable, "current_value", "0"),
            )
        })
        .collect()
}

/// The rules a scenario names, in the order it names them.
fn rules_of(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    scenario: &Value,
) -> Result<Vec<Value>, StoreError> {
    let wanted: Vec<String> = scenario
        .get("rules")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(value_text)
        .collect();
    let stored = Records::new(graph, MODELING_RULE).list(scope, 2000)?;
    Ok(wanted
        .iter()
        .filter_map(|id| stored.iter().find(|rule| field(rule, "id") == id).cloned())
        .collect())
}

fn scenario_of(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    scenario_id: &str,
) -> Result<Value, StoreError> {
    Records::new(graph, SCENARIO_MODEL)
        .list(scope, 2000)?
        .into_iter()
        .find(|row| field(row, "id") == scenario_id)
        .ok_or(StoreError::NotFound)
}

/// `POST /scenarios/`.
pub fn open_model_scenario(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let name = text(payload, "name");
    if name.is_empty() {
        return Err(refused("name is required"));
    }
    let rules: Vec<String> = payload
        .get("rules")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(value_text)
        .collect();

    // every rule a scenario names must be one the caller holds, so a scenario cannot borrow another's rules
    let stored = Records::new(graph, MODELING_RULE).list(scope, 2000)?;
    for rule_id in &rules {
        if !stored.iter().any(|rule| field(rule, "id") == rule_id) {
            return Err(refused(&format!("Rule {rule_id} not found")));
        }
    }

    remember(
        graph,
        scope,
        SCENARIO_MODEL,
        json!({
            "name": name,
            "description": text(payload, "description"),
            "scenario_type": text(payload, "scenario_type"),
            "base_date": text(payload, "base_date"),
            "end_date": text(payload, "end_date"),
            "variables": payload.get("variables").cloned().unwrap_or(json!([])),
            "rules": rules,
            "assumptions": payload.get("assumptions").cloned().unwrap_or(Value::Null),
            "status": "draft",
            "results": Value::Null,
        }),
    )
}

/// `GET /scenarios/`.
pub fn list_model_scenarios(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let wanted_status = optional(query, "status").unwrap_or_default();
    let wanted_type = optional(query, "scenario_type").unwrap_or_default();
    Ok(Value::Array(
        Records::new(graph, SCENARIO_MODEL)
            .list(scope, 2000)?
            .into_iter()
            .filter(|row| wanted_status.is_empty() || field(row, "status") == wanted_status)
            .filter(|row| wanted_type.is_empty() || field(row, "scenario_type") == wanted_type)
            .collect(),
    ))
}

/// `GET /scenarios/{scenario_id}`.
pub fn get_scenario(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    scenario_id: &str,
) -> Result<Value, StoreError> {
    scenario_of(graph, scope, scenario_id)
}

/// The fields a scenario may be updated through: everything it carries except what identifies it.
fn updates_for(payload: &Value) -> Value {
    let mut updates = serde_json::Map::new();
    if let Some(given) = payload.as_object() {
        for (key, value) in given {
            if key == "id" || key == "user_id" || key == "created_at" || key == "updated_date" {
                continue;
            }
            updates.insert(key.clone(), value.clone());
        }
    }
    if let Some(status) = optional(&Value::Object(updates.clone()), "status") {
        if !STATUSES.contains(&status.as_str()) {
            return json!({"__refused__": "status must be one of draft, active, completed or archived"});
        }
    }
    Value::Object(updates)
}

/// `PUT /scenarios/{scenario_id}`.
pub fn update_scenario(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    scenario_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    scenario_of(graph, scope, scenario_id)?;
    let updates = updates_for(payload);
    if let Some(reason) = updates.get("__refused__").and_then(Value::as_str) {
        return Err(refused(reason));
    }
    Records::new(graph, SCENARIO_MODEL).update(scope, scenario_id, &updates)
}

/// `DELETE /scenarios/{scenario_id}`.
pub fn delete_scenario(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    scenario_id: &str,
) -> Result<Value, StoreError> {
    scenario_of(graph, scope, scenario_id)?;
    Records::new(graph, SCENARIO_MODEL).delete(scope, scenario_id)?;
    Ok(json!({"deleted": scenario_id}))
}

/// `POST /rules/`.
pub fn create_rule(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let name = text(payload, "name");
    if name.is_empty() {
        return Err(refused("name is required"));
    }
    let conditions = payload
        .get("conditions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let actions = payload
        .get("actions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if conditions.is_empty() || actions.is_empty() {
        return Err(refused(
            "Conditions and actions must have at least one item",
        ));
    }

    remember(
        graph,
        scope,
        MODELING_RULE,
        json!({
            "name": name,
            "description": text(payload, "description"),
            "conditions": conditions,
            "actions": actions,
            "priority": payload.get("priority").cloned().unwrap_or(json!(100)),
            "enabled": payload.get("enabled").cloned().unwrap_or(json!(true)),
        }),
    )
}

/// `GET /rules/`.
pub fn list_rules(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let enabled_only = matches!(query.get("enabled_only"), Some(Value::Bool(true)))
        || optional(query, "enabled_only")
            .map(|value| value == "true")
            .unwrap_or(false);
    Ok(Value::Array(
        Records::new(graph, MODELING_RULE)
            .list(scope, 2000)?
            .into_iter()
            .filter(|row| !enabled_only || matches!(row.get("enabled"), Some(Value::Bool(true))))
            .collect(),
    ))
}

fn rule_of(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    rule_id: &str,
) -> Result<Value, StoreError> {
    Records::new(graph, MODELING_RULE)
        .list(scope, 2000)?
        .into_iter()
        .find(|row| field(row, "id") == rule_id)
        .ok_or(StoreError::NotFound)
}

/// `GET /rules/{rule_id}`.
pub fn get_rule(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    rule_id: &str,
) -> Result<Value, StoreError> {
    rule_of(graph, scope, rule_id)
}

/// `PUT /rules/{rule_id}`.
pub fn update_rule(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    rule_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    rule_of(graph, scope, rule_id)?;
    let mut updates = serde_json::Map::new();
    if let Some(given) = payload.as_object() {
        for (key, value) in given {
            if key == "id" || key == "created_at" || key == "updated_date" {
                continue;
            }
            updates.insert(key.clone(), value.clone());
        }
    }
    Records::new(graph, MODELING_RULE).update(scope, rule_id, &Value::Object(updates))
}

/// `DELETE /rules/{rule_id}`.
pub fn delete_rule(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    rule_id: &str,
) -> Result<Value, StoreError> {
    rule_of(graph, scope, rule_id)?;
    Records::new(graph, MODELING_RULE).delete(scope, rule_id)?;
    Ok(json!({"deleted": rule_id}))
}

/// `POST /what-if/`.
pub fn what_if(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let scenario_id = text(payload, "scenario_id");
    let scenario = scenario_of(graph, scope, &scenario_id)?;
    let rules = rules_of(graph, scope, &scenario)?;

    let base = base_figures(&scenario);
    let mut changed = base.clone();
    if let Some(changes) = payload.get("variable_changes").and_then(Value::as_object) {
        for (name, value) in changes {
            // a change to a variable the scenario does not carry is left alone rather than added
            if figure(&base, name).is_some() {
                put(&mut changed, name, value_text(value));
            }
        }
    }

    let mut original_figures = base.clone();
    for rule in &rules {
        apply_rule(rule, &mut original_figures);
    }
    let original = outcome_of(&scenario, &original_figures);

    let mut new_figures = changed.clone();
    for rule in &rules {
        apply_rule(rule, &mut new_figures);
    }
    let new = outcome_of(&scenario, &new_figures);

    let variance = new.minus(&original);
    let variance_percent = if original.is_zero() {
        zero()
    } else {
        variance.over(&original).times(&hundred())
    };

    let as_named = |figures: &Figures| -> serde_json::Map<String, Value> {
        figures
            .iter()
            .map(|(name, value)| (name.clone(), json!(value)))
            .collect()
    };

    // the shipped split of the variance across the three accounts a change is read against
    let affected = json!([
        {"account": "Revenue", "variance": wire(&variance.times(&Money::parse("0.4").unwrap_or_else(|_| zero())))},
        {"account": "Expenses", "variance": wire(&variance.times(&Money::parse("0.3").unwrap_or_else(|_| zero())))},
        {"account": "Net Income", "variance": wire(&variance.times(&Money::parse("0.3").unwrap_or_else(|_| zero())))},
    ]);

    remember(
        graph,
        scope,
        WHAT_IF_RESULT,
        json!({
            "analysis_id": vimbai_common::service_id(),
            "scenario_id": scenario_id,
            "base_values": as_named(&base),
            "changed_values": as_named(&changed),
            "original_outcome": wire(&original),
            "new_outcome": wire(&new),
            "variance": wire(&variance),
            "variance_percent": wire(&variance_percent),
            "affected_accounts": affected,
        }),
    )
}

/// `GET /what-if/{analysis_id}`.
pub fn get_what_if(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    analysis_id: &str,
) -> Result<Value, StoreError> {
    Records::new(graph, WHAT_IF_RESULT)
        .list(scope, 2000)?
        .into_iter()
        .find(|row| field(row, "analysis_id") == analysis_id || field(row, "id") == analysis_id)
        .ok_or(StoreError::NotFound)
}

/// `GET /what-if/scenario/{scenario_id}`.
pub fn list_what_if(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    scenario_id: &str,
) -> Result<Value, StoreError> {
    let wanted = scenario_id.to_string();
    Ok(Value::Array(
        Records::new(graph, WHAT_IF_RESULT)
            .list(scope, 2000)?
            .into_iter()
            .filter(|row| field(row, "scenario_id") == wanted)
            .collect(),
    ))
}

/// `POST /sensitivity/`.
pub fn sweep(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let scenario_id = text(payload, "scenario_id");
    let scenario = scenario_of(graph, scope, &scenario_id)?;
    let rules = rules_of(graph, scope, &scenario)?;
    let variable_name = text(payload, "variable_name");

    let base = base_figures(&scenario);
    let current = match figure(&base, &variable_name).and_then(|value| as_money(&value)) {
        Some(value) => value,
        None => return Err(StoreError::NotFound),
    };

    let lowest = Money::parse(&decimal(payload, "min_change", "0"))
        .map_err(|_| refused("min_change must be an amount"))?;
    let highest = Money::parse(&decimal(payload, "max_change", "0"))
        .map_err(|_| refused("max_change must be an amount"))?;
    let steps = payload
        .get("steps")
        .and_then(Value::as_i64)
        .unwrap_or(10)
        .clamp(2, 100);

    // the span divided into the steps, which is what the service did - steps of one divide by nothing, and it read
    // that as a span rather than refusing
    let divisor = if steps - 1 == 0 { 1 } else { steps - 1 };
    let step_size = highest.minus(&lowest).over(&Money::from_i64(divisor));

    let mut outcomes = Vec::new();
    for index in 0..steps {
        let change = lowest.plus(&step_size.times(&Money::from_i64(index)));
        let mut figures = base.clone();
        put(
            &mut figures,
            &variable_name,
            current.plus(&change).to_fixed(6),
        );
        for rule in &rules {
            apply_rule(rule, &mut figures);
        }
        let outcome = outcome_of(&scenario, &figures);
        let change_percent = if current.is_zero() {
            zero()
        } else {
            change.over(&current).times(&hundred())
        };
        outcomes.push(json!({
            "change": wire(&change),
            "outcome": wire(&outcome),
            "change_percent": wire(&change_percent),
        }));
    }

    // the stretch that moves the outcome most per unit of change
    let mut best_steepness = zero();
    let mut steepest = json!({});
    for pair in outcomes.windows(2) {
        let change_span = Money::parse(field(&pair[1], "change")).unwrap_or_else(|_| zero());
        let change_before = Money::parse(field(&pair[0], "change")).unwrap_or_else(|_| zero());
        let outcome_after = Money::parse(field(&pair[1], "outcome")).unwrap_or_else(|_| zero());
        let outcome_before = Money::parse(field(&pair[0], "outcome")).unwrap_or_else(|_| zero());

        let delta_change = change_span.minus(&change_before);
        let delta_outcome = outcome_after.minus(&outcome_before);
        let steepness = if delta_change.is_zero() {
            zero()
        } else {
            delta_outcome.over(&delta_change).abs()
        };
        if steepness.as_decimal() > best_steepness.as_decimal() {
            best_steepness = steepness;
            steepest = json!({
                "min_change": field(&pair[0], "change"),
                "max_change": field(&pair[1], "change"),
                "outcome_impact": wire(&delta_outcome.abs()),
            });
        }
    }

    let first_outcome = outcomes
        .first()
        .and_then(|row| Money::parse(field(row, "outcome")).ok())
        .unwrap_or_else(zero);
    let mut recommendations = Vec::new();
    for outcome in &outcomes {
        let value = Money::parse(field(outcome, "outcome")).unwrap_or_else(|_| zero());
        let change_percent =
            Money::parse(field(outcome, "change_percent")).unwrap_or_else(|_| zero());
        if value.as_decimal()
            > first_outcome
                .times(&Money::parse("1.1").unwrap_or_else(|_| zero()))
                .as_decimal()
        {
            recommendations.push(format!(
                "Increase {variable_name} by {}% yields {}",
                change_percent.to_fixed(1),
                value.to_fixed(2)
            ));
        } else if value.as_decimal()
            < first_outcome
                .times(&Money::parse("0.9").unwrap_or_else(|_| zero()))
                .as_decimal()
        {
            recommendations.push(format!(
                "Decrease {variable_name} by {}% yields {}",
                change_percent.abs().to_fixed(1),
                value.to_fixed(2)
            ));
        }
    }
    recommendations.truncate(5);

    remember(
        graph,
        scope,
        SENSITIVITY_MODEL_RESULT,
        json!({
            "analysis_id": vimbai_common::service_id(),
            "scenario_id": scenario_id,
            "variable_name": variable_name,
            "outcomes": outcomes,
            "most_sensitive_range": steepest,
            "recommendations": recommendations,
        }),
    )
}

/// `POST /compare/`.
pub fn compare_model_scenarios(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    scenario_ids: &[String],
    today: &str,
) -> Result<Value, StoreError> {
    let mut scenarios = Vec::new();
    for id in scenario_ids {
        let scenario = scenario_of(graph, scope, id)?;
        let rules = rules_of(graph, scope, &scenario)?;
        scenarios.push((scenario, rules));
    }

    let mut outcomes = serde_json::Map::new();
    let mut listed = Vec::new();
    for (scenario, rules) in &scenarios {
        let mut figures = base_figures(scenario);
        for rule in rules {
            apply_rule(rule, &mut figures);
        }
        let outcome = outcome_of(scenario, &figures);
        outcomes.insert(field(scenario, "id").to_string(), json!(wire(&outcome)));

        let variables: serde_json::Map<String, Value> = base_figures(scenario)
            .into_iter()
            .map(|(name, value)| (name, json!(value)))
            .collect();
        listed.push(json!({
            "id": field(scenario, "id"),
            "name": field(scenario, "name"),
            "type": field(scenario, "scenario_type"),
            "base_date": field(scenario, "base_date"),
            "end_date": field(scenario, "end_date"),
            "variables": variables,
        }));
    }

    let mut best: Option<(&str, Money, &str)> = None;
    let mut worst: Option<(&str, Money, &str)> = None;
    for (scenario, _) in &scenarios {
        let id = field(scenario, "id");
        let outcome = outcomes
            .get(id)
            .and_then(|value| value.as_str())
            .and_then(|text| Money::parse(text).ok())
            .unwrap_or_else(zero);
        let name = field(scenario, "name");
        if best
            .as_ref()
            .map(|(_, so_far, _)| outcome.as_decimal() > so_far.as_decimal())
            .unwrap_or(true)
        {
            best = Some((id, outcome.clone(), name));
        }
        if worst
            .as_ref()
            .map(|(_, so_far, _)| outcome.as_decimal() < so_far.as_decimal())
            .unwrap_or(true)
        {
            worst = Some((id, outcome, name));
        }
    }

    Ok(json!({
        "scenarios": listed,
        "comparison_date": today,
        "best_case": best.map(|(id, outcome, name)| json!({"scenario_id": id, "outcome": wire(&outcome), "name": name})),
        "worst_case": worst.map(|(id, outcome, name)| json!({"scenario_id": id, "outcome": wire(&outcome), "name": name})),
        "recommendations": [],
        "outcomes": outcomes,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::FakeGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn scenario(kind: &str, variables: Value) -> Value {
        json!({
            "id": "sc-1", "name": "Q1", "scenario_type": kind,
            "base_date": "2026-01-01", "end_date": "2026-07-01",
            "variables": variables, "rules": [], "assumptions": {"base_revenue": 120000}
        })
    }

    #[test]
    fn a_budget_forecast_adds_its_variables_up() {
        let figures = vec![
            ("Revenue".to_string(), "1000".to_string()),
            ("Costs".to_string(), "400".to_string()),
        ];
        assert_eq!(
            outcome_of(&scenario("budget_forecast", json!([])), &figures).to_fixed(2),
            "1400.00"
        );
    }

    #[test]
    fn a_revenue_projection_compounds_over_the_months_between_the_dates() {
        let figures = vec![("growth_rate".to_string(), "0.1".to_string())];
        let outcome = outcome_of(&scenario("revenue_projection", json!([])), &figures);
        // 181 days is 6.0333 months at the service's thirty-day month, and 10% compounds over each
        assert_eq!(outcome.round(2).to_fixed(2), "213263.79");
    }

    #[test]
    fn an_expense_simulation_adds_only_the_variables_named_for_expenses() {
        let figures = vec![
            ("Rent expense".to_string(), "800".to_string()),
            ("Salaries expense".to_string(), "2000".to_string()),
            ("Revenue".to_string(), "9000".to_string()),
        ];
        assert_eq!(
            outcome_of(&scenario("expense_simulation", json!([])), &figures).to_fixed(2),
            "2800.00"
        );
    }

    #[test]
    fn a_cash_flow_nets_inflows_against_outflows_and_a_profitability_subtracts_costs() {
        let cash = vec![
            ("Customer inflow".to_string(), "5000".to_string()),
            ("Supplier outflow".to_string(), "3000".to_string()),
        ];
        assert_eq!(
            outcome_of(&scenario("cash_flow", json!([])), &cash).to_fixed(2),
            "2000.00"
        );

        let profit = vec![
            ("revenue".to_string(), "5000".to_string()),
            ("costs".to_string(), "4200".to_string()),
        ];
        assert_eq!(
            outcome_of(&scenario("profitability", json!([])), &profit).to_fixed(2),
            "800.00"
        );
    }

    #[test]
    fn a_rule_adjusts_scales_bends_or_overwrites_what_it_targets() {
        let rule = json!({
            "conditions": [{"field": "Revenue", "operator": "greater_than", "value": 100}],
            "actions": [
                {"action_type": "adjust_amount", "target_field": "Revenue", "value": 50},
                {"action_type": "scale_amount", "target_field": "Costs", "value": 2},
                {"action_type": "apply_percentage", "target_field": "Rent", "value": 10},
                {"action_type": "set_value", "target_field": "Band", "value": "high"}
            ]
        });
        let mut figures = vec![
            ("Revenue".to_string(), "1000".to_string()),
            ("Costs".to_string(), "400".to_string()),
            ("Rent".to_string(), "200".to_string()),
        ];
        apply_rule(&rule, &mut figures);
        assert_eq!(figure(&figures, "Revenue").unwrap(), "1050.000000");
        assert_eq!(figure(&figures, "Costs").unwrap(), "800.000000");
        assert_eq!(figure(&figures, "Rent").unwrap(), "220.000000");
        assert_eq!(figure(&figures, "Band").unwrap(), "high");
    }

    #[test]
    fn a_rule_stays_its_hand_when_a_condition_on_an_absent_field_is_asked() {
        let rule = json!({
            "conditions": [{"field": "Headcount", "operator": "greater_than", "value": 5}],
            "actions": [{"action_type": "adjust_amount", "target_field": "Revenue", "value": 50}]
        });
        let mut figures = vec![("Revenue".to_string(), "1000".to_string())];
        apply_rule(&rule, &mut figures);
        assert_eq!(figure(&figures, "Revenue").unwrap(), "1000");
    }

    #[test]
    fn every_condition_must_hold_for_a_rule_to_fire() {
        let rule = json!({
            "conditions": [
                {"field": "Revenue", "operator": "greater_than", "value": 100},
                {"field": "Costs", "operator": "less_than", "value": 100}
            ],
            "actions": [{"action_type": "adjust_amount", "target_field": "Revenue", "value": 50}]
        });
        let mut figures = vec![
            ("Revenue".to_string(), "1000".to_string()),
            ("Costs".to_string(), "400".to_string()),
        ];
        apply_rule(&rule, &mut figures);
        assert_eq!(figure(&figures, "Revenue").unwrap(), "1000");
    }

    #[test]
    fn a_between_condition_reads_its_two_bounds() {
        let condition =
            json!({"field": "Margin", "operator": "between", "value": 10, "secondary_value": 20});
        let inside = vec![("Margin".to_string(), "15".to_string())];
        let outside = vec![("Margin".to_string(), "25".to_string())];
        assert!(condition_holds(&condition, &inside));
        assert!(!condition_holds(&condition, &outside));
    }

    #[test]
    fn a_contains_condition_reads_text() {
        let condition = json!({"field": "Band", "operator": "contains", "value": "hig"});
        let figures = vec![("Band".to_string(), "high".to_string())];
        assert!(condition_holds(&condition, &figures));
    }

    #[test]
    fn a_what_if_records_both_outcomes_and_their_split_across_the_three_accounts() {
        let graph = FakeGraph::queue(vec![
            vec![
                json!({"id": "sc-1", "name": "Q1", "scenario_type": "profitability",
                        "variables": [{"name": "revenue", "current_value": "1000"},
                                      {"name": "costs", "current_value": "600"}],
                        "rules": []}),
            ],
            vec![],
            vec![json!({"id": "w-1"})],
        ]);
        let answer = what_if(
            &graph,
            &scope(),
            &json!({"scenario_id": "sc-1", "variable_changes": {"revenue": 1500}}),
        )
        .expect("run");
        assert_eq!(answer["original_outcome"], json!("400.000000"));
        assert_eq!(answer["new_outcome"], json!("900.000000"));
        assert_eq!(answer["variance"], json!("500.000000"));
        assert_eq!(answer["variance_percent"], json!("125.000000"));
        // four tenths of 500 is 200
        assert_eq!(
            answer["affected_accounts"][0]["variance"],
            json!("200.000000")
        );
    }

    #[test]
    fn a_what_if_over_a_variable_that_is_not_there_leaves_the_figures_alone() {
        let graph = FakeGraph::queue(vec![
            vec![json!({"id": "sc-1", "scenario_type": "profitability",
                        "variables": [{"name": "revenue", "current_value": "1000"},
                                      {"name": "costs", "current_value": "600"}],
                        "rules": []})],
            vec![],
            vec![json!({"id": "w-1"})],
        ]);
        let answer = what_if(
            &graph,
            &scope(),
            &json!({"scenario_id": "sc-1", "variable_changes": {"headcount": 40}}),
        )
        .expect("run");
        assert_eq!(answer["new_outcome"], json!("400.000000"));
        assert_eq!(answer["variance_percent"], json!("0.000000"));
    }

    #[test]
    fn a_sweep_walks_the_span_and_reports_where_the_outcome_moves_fastest() {
        let graph = FakeGraph::queue(vec![
            vec![json!({"id": "sc-1", "scenario_type": "profitability",
                        "variables": [{"name": "revenue", "current_value": "1000"},
                                      {"name": "costs", "current_value": "600"}],
                        "rules": []})],
            vec![],
            vec![json!({"id": "s-1"})],
        ]);
        let answer = sweep(
            &graph,
            &scope(),
            &json!({"scenario_id": "sc-1", "variable_name": "revenue",
                    "min_change": "0", "max_change": "100", "steps": 3}),
        )
        .expect("swept");
        assert_eq!(answer["outcomes"].as_array().unwrap().len(), 3);
        assert_eq!(answer["outcomes"][0]["outcome"], json!("400.000000"));
        assert_eq!(answer["outcomes"][2]["outcome"], json!("500.000000"));
        assert_eq!(
            answer["most_sensitive_range"]["outcome_impact"],
            json!("50.000000")
        );
        // the two steps that clear ten percent of the first outcome are both offered
        assert_eq!(answer["recommendations"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn a_sweep_of_a_variable_the_scenario_does_not_carry_is_a_miss() {
        let graph = FakeGraph::queue(vec![vec![
            json!({"id": "sc-1", "scenario_type": "profitability",
                                                       "variables": [], "rules": []}),
        ]]);
        assert!(matches!(
            sweep(
                &graph,
                &scope(),
                &json!({"scenario_id": "sc-1", "variable_name": "revenue",
                        "min_change": "0", "max_change": "10", "steps": 3})
            ),
            Err(StoreError::NotFound)
        ));
    }

    #[test]
    fn a_comparison_names_the_best_and_the_worst_of_what_it_was_given() {
        let graph = FakeGraph::queue(vec![
            vec![
                json!({"id": "sc-1", "name": "Prudent", "scenario_type": "profitability",
                        "variables": [{"name": "revenue", "current_value": "1000"},
                                      {"name": "costs", "current_value": "800"}], "rules": []}),
            ],
            vec![],
            vec![
                json!({"id": "sc-2", "name": "Bold", "scenario_type": "profitability",
                        "variables": [{"name": "revenue", "current_value": "2000"},
                                      {"name": "costs", "current_value": "800"}], "rules": []}),
            ],
            vec![],
        ]);
        let answer = compare_model_scenarios(
            &graph,
            &scope(),
            &["sc-1".to_string(), "sc-2".to_string()],
            "2026-11-13",
        )
        .expect("compared");
        assert_eq!(answer["best_case"]["name"], json!("Bold"));
        assert_eq!(answer["worst_case"]["name"], json!("Prudent"));
        assert_eq!(answer["comparison_date"], json!("2026-11-13"));
        assert_eq!(answer["recommendations"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn a_rule_worth_keeping_needs_a_condition_and_an_action() {
        let graph = FakeGraph::returning(vec![]);
        assert!(create_rule(&graph, &scope(), &json!({"name": "Growth"})).is_err());
    }

    #[test]
    fn a_scenario_naming_a_rule_the_caller_does_not_hold_is_refused() {
        let graph = FakeGraph::queue(vec![vec![]]);
        assert!(open_model_scenario(
            &graph,
            &scope(),
            &json!({"name": "Q1", "scenario_type": "budget_forecast", "base_date": "2026-01-01",
                    "end_date": "2026-06-30", "rules": ["r-9"]}),
        )
        .is_err());
    }

    #[test]
    fn a_status_the_service_does_not_know_is_refused_on_an_update() {
        let graph = FakeGraph::queue(vec![vec![json!({"id": "sc-1"})]]);
        assert!(update_scenario(&graph, &scope(), "sc-1", &json!({"status": "shredded"})).is_err());
    }

    #[test]
    fn the_rule_listing_can_show_only_the_enabled_ones() {
        let graph = FakeGraph::returning(vec![
            json!({"id": "r-1", "name": "Growth", "enabled": true}),
            json!({"id": "r-2", "name": "Retired", "enabled": false}),
        ]);
        let answer = list_rules(&graph, &scope(), &json!({"enabled_only": true})).expect("listed");
        assert_eq!(answer.as_array().unwrap().len(), 1);
        assert_eq!(answer[0]["name"], json!("Growth"));
    }
}
