#![forbid(unsafe_code)]
//! sensitivity-analysis-service: how much the target moves when one variable is nudged.
//!
//! The elasticity the service reports is the step taken over the relative move in the target, taken as a size. A step
//! that leaves the target where it was has no elasticity rather than an infinite one.
//!
//! The impact of a step is not measured: the service estimated it from the variable's *name*, reading words like
//! "revenue", "cost" or "interest" and applying a fixed share. That is a lookup table, not a measurement, and it is
//! kept as shipped so the answers stay comparable - a name carrying "revenue" moves a profit target by six tenths of
//! the step, one carrying "cost" by four tenths the other way, and a name carrying neither by a fifth.
//!
//! Behaviours kept as shipped, each pinned by a test:
//!
//! * every variable is swept across every step, so the number of results is the variables times the steps;
//! * the changed value is the variable's base moved by the step, whether or not it is a sensible value;
//! * the elasticity is a size, and nil when the step did not move the target;
//! * the most sensitive variable is the one with the highest average elasticity, and nothing when nothing moved -
//!   except that the service divided the step by the move rather than the move by the step, so its elasticity is the
//!   reciprocal of the usual one and the name it picks is the variable that shifts the target *least*. Ported as it
//!   stands, because the answers are what callers see, and pinned by a test so nobody reads the label as a measurement;
//! * a completed analysis is kept, and the listing is by company.

use crate::figures::{amount, hundred, one};
use std::collections::BTreeMap;

use serde_json::{json, Value};

use vimbai_common::Money;
use vimbai_store::amounts::wire;
use vimbai_store::read::text;
use vimbai_store::write::remember;
use vimbai_store::{BookScope, Graph, Records, StoreError, SENSITIVITY_ANALYSIS};

/// How much a step in a variable moves the target, by the name the variable carries.
fn estimate_impact(name: &str, step: &Money, target: &str, base_target: &Money) -> Money {
    let change = step.over(&hundred());
    let named = name.to_lowercase();

    let share = if target == "net_profit" || target == "profit" {
        if named.contains("revenue") {
            change.times(&Money::parse("0.6").unwrap_or_else(|_| Money::zero()))
        } else if named.contains("cost") || named.contains("expense") {
            change.times(&Money::parse("-0.4").unwrap_or_else(|_| Money::zero()))
        } else if named.contains("interest") {
            change.times(&Money::parse("-0.1").unwrap_or_else(|_| Money::zero()))
        } else {
            change.times(&Money::parse("0.2").unwrap_or_else(|_| Money::zero()))
        }
    } else if target == "cash_flow" || target == "cashflow" {
        if named.contains("revenue") {
            change.times(&Money::parse("0.7").unwrap_or_else(|_| Money::zero()))
        } else if named.contains("cost") {
            change.times(&Money::parse("-0.5").unwrap_or_else(|_| Money::zero()))
        } else {
            change.times(&Money::parse("0.3").unwrap_or_else(|_| Money::zero()))
        }
    } else if target == "revenue" {
        if named.contains("price") {
            change.times(&Money::parse("0.8").unwrap_or_else(|_| Money::zero()))
        } else if named.contains("volume") || named.contains("sales") {
            change.times(&Money::parse("0.9").unwrap_or_else(|_| Money::zero()))
        } else {
            change.times(&Money::parse("0.3").unwrap_or_else(|_| Money::zero()))
        }
    } else {
        change.times(&Money::parse("0.2").unwrap_or_else(|_| Money::zero()))
    };

    base_target.times(&one().plus(&share))
}

/// `POST /analyze`.
pub fn analyze(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let target = text(payload, "target_metric");
    let base_target = amount(payload, "base_target_value", "0")?;
    let variables = payload
        .get("variables")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let steps: Vec<Money> = payload
        .get("change_steps")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_else(|| vec![json!(-10), json!(-5), json!(0), json!(5), json!(10)])
        .iter()
        .map(|step| {
            Money::parse(step.to_string().trim_matches('"')).unwrap_or_else(|_| Money::zero())
        })
        .collect();

    let mut results = Vec::new();
    let mut swings: BTreeMap<String, (Money, i64)> = BTreeMap::new();

    for variable in &variables {
        let name = text(variable, "name");
        let base = amount(variable, "base_value", "0")?;
        for step in &steps {
            let changed = base.times(&one().plus(&step.over(&hundred())));
            let impact = estimate_impact(&name, step, &target, &base_target);

            // the step over the relative move in the target, as a size, and nil when the target held
            let elasticity = if impact.as_decimal() == base_target.as_decimal() {
                Money::zero()
            } else {
                let floor = if base_target.as_decimal() < one().as_decimal() {
                    one()
                } else {
                    base_target.clone()
                };
                let moved = impact.minus(&base_target).over(&floor);
                step.over(&hundred()).over(&moved).abs()
            };

            results.push(json!({
                "variable_name": name,
                "base_value": wire(&base),
                "changed_value": wire(&changed),
                "change_pct": wire(step),
                "impact_on_target": wire(&impact),
                "elasticity": wire(&elasticity),
            }));

            let entry = swings.entry(name.clone()).or_insert((Money::zero(), 0));
            entry.0 = entry.0.plus(&elasticity);
            entry.1 += 1;
        }
    }

    let most_sensitive = swings
        .iter()
        .max_by(|left, right| {
            let left_average = left.1 .0.over(&Money::from_i64(left.1 .1));
            let right_average = right.1 .0.over(&Money::from_i64(right.1 .1));
            left_average
                .as_decimal()
                .cmp(right_average.as_decimal())
                // the first name wins a tie, since the original took the first key at the maximum
                .then_with(|| right.0.cmp(left.0))
        })
        .map(|(name, _)| name.clone())
        .unwrap_or_default();

    remember(
        graph,
        scope,
        SENSITIVITY_ANALYSIS,
        json!({
            "company_id": text(payload, "company_id"),
            "target_metric": target,
            "base_target_value": wire(&base_target),
            "results": results,
            "most_sensitive_variable": most_sensitive,
        }),
    )
}

/// `GET /analyses/{company_id}`.
pub fn list_analyses(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    company_id: &str,
) -> Result<Value, StoreError> {
    let wanted = company_id.to_string();
    Ok(Value::Array(
        Records::new(graph, SENSITIVITY_ANALYSIS)
            .list(scope, 2000)?
            .into_iter()
            .filter(|row| vimbai_store::read::field(row, "company_id") == wanted)
            .collect(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::FakeGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    #[test]
    fn every_variable_is_swept_across_every_step() {
        let graph = FakeGraph::returning(vec![json!({"id": "a-1"})]);
        let answer = analyze(
            &graph,
            &scope(),
            &json!({
                "company_id": "c-1", "target_metric": "net_profit", "base_target_value": "1000",
                "variables": [{"name": "Revenue", "base_value": "5000"},
                              {"name": "Materials cost", "base_value": "2000"}],
                "change_steps": [-10, 0, 10]
            }),
        )
        .expect("analysed");
        assert_eq!(answer["results"].as_array().unwrap().len(), 6);
        assert_eq!(answer["results"][0]["variable_name"], json!("Revenue"));
        assert_eq!(answer["results"][0]["change_pct"], json!("-10.000000"));
    }

    #[test]
    fn a_revenue_step_moves_a_profit_target_by_six_tenths() {
        let graph = FakeGraph::returning(vec![json!({"id": "a-1"})]);
        let answer = analyze(
            &graph,
            &scope(),
            &json!({
                "company_id": "c-1", "target_metric": "net_profit", "base_target_value": "1000",
                "variables": [{"name": "Revenue", "base_value": "5000"}],
                "change_steps": [10]
            }),
        )
        .expect("analysed");
        // a tenth of 1,000 at six tenths is 60, so 1,060 against a base of 1,000
        assert_eq!(
            answer["results"][0]["impact_on_target"],
            json!("1060.000000")
        );
        // the step over the move: 0.1 over 0.06
        assert_eq!(answer["results"][0]["elasticity"], json!("1.666667"));
    }

    #[test]
    fn a_cost_step_moves_a_profit_target_the_other_way() {
        let graph = FakeGraph::returning(vec![json!({"id": "a-1"})]);
        let answer = analyze(
            &graph,
            &scope(),
            &json!({
                "company_id": "c-1", "target_metric": "net_profit", "base_target_value": "1000",
                "variables": [{"name": "Materials cost", "base_value": "2000"}],
                "change_steps": [10]
            }),
        )
        .expect("analysed");
        // four tenths of 100 the other way
        assert_eq!(
            answer["results"][0]["impact_on_target"],
            json!("960.000000")
        );
        // the elasticity is taken as a size, so a fall reads positive
        assert_eq!(answer["results"][0]["elasticity"], json!("2.500000"));
    }

    #[test]
    fn a_step_that_leaves_the_target_where_it_was_has_no_elasticity() {
        let graph = FakeGraph::returning(vec![json!({"id": "a-1"})]);
        let answer = analyze(
            &graph,
            &scope(),
            &json!({
                "company_id": "c-1", "target_metric": "net_profit", "base_target_value": "1000",
                "variables": [{"name": "Revenue", "base_value": "5000"}],
                "change_steps": [0]
            }),
        )
        .expect("analysed");
        assert_eq!(answer["results"][0]["elasticity"], json!("0.000000"));
    }

    #[test]
    fn the_most_sensitive_variable_is_the_one_that_moves_the_target_most_on_average() {
        let graph = FakeGraph::returning(vec![json!({"id": "a-1"})]);
        let answer = analyze(
            &graph,
            &scope(),
            &json!({
                "company_id": "c-1", "target_metric": "revenue", "base_target_value": "1000",
                "variables": [{"name": "Unit volume", "base_value": "100"},
                              {"name": "Headcount", "base_value": "10"}],
                "change_steps": [10]
            }),
        )
        .expect("analysed");
        // volume moves revenue by nine tenths where headcount moves it by three, and the reciprocal the service used
        // makes headcount the larger figure, so the label points at the variable that moves the target least
        assert_eq!(answer["most_sensitive_variable"], json!("Headcount"));
    }

    #[test]
    fn the_sweep_needs_a_target_and_a_base_to_measure_against() {
        let graph = FakeGraph::returning(vec![json!({"id": "a-1"})]);
        let answer = analyze(
            &graph,
            &scope(),
            &json!({"company_id": "c-1", "target_metric": "revenue", "base_target_value": "0",
                    "variables": [{"name": "Price", "base_value": "10"}], "change_steps": [10]}),
        )
        .expect("analysed");
        // a base of nothing is measured against one, so the move is the whole impact
        assert_eq!(answer["results"][0]["impact_on_target"], json!("0.000000"));
        assert_eq!(answer["results"][0]["elasticity"], json!("0.000000"));
    }

    #[test]
    fn the_listing_is_by_company() {
        let graph = FakeGraph::returning(vec![
            json!({"id": "a-1", "company_id": "c-1"}),
            json!({"id": "a-2", "company_id": "c-2"}),
        ]);
        let answer = list_analyses(&graph, &scope(), "c-2").expect("listed");
        assert_eq!(answer.as_array().unwrap().len(), 1);
    }
}
