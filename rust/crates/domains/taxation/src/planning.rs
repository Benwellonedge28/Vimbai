#![forbid(unsafe_code)]
//! tax-planning-service: the strategies a tax planner submits, and the plan they add up to.
//!
//! A plan is arithmetic on what the caller states, so nothing about it is stored: `/plan` answers from the
//! request alone, which is what the shipped service did.
//!
//! Behaviours kept as shipped, each pinned by a test:
//!
//! * projected tax never goes below zero - savings larger than the tax bill plan a nil bill, not a refund;
//! * a strategy's return is its net benefit over what it costs, and a strategy that costs nothing has no return
//!   to state rather than an infinite one, so the answer is `null` and the strategy is not recommended;
//! * a strategy is only recommended when it returns more than it costs and is low or medium risk - a high-risk
//!   saving is measured but never advised;
//! * the recommended list carries names, not identifiers, because it is read as advice;
//! * figures are rounded to the cent, and the rounding is applied to the answer rather than to the running total;
//! * a plan carries an identifier even though nothing is stored, so a caller can refer to the answer they were
//!   given.

use serde_json::{json, Value};

use vimbai_common::ids::service_id;
use vimbai_common::Money;
use vimbai_store::amounts::wire;
use vimbai_store::read::{field, money_or_nothing, optional, text, whole_signed};
use vimbai_store::write::remember;
use vimbai_store::{BookScope, Graph, Records, StoreError, TAX_STRATEGY};

const LISTED: usize = 2000;

fn strategy_in(payload: &Value) -> Result<Value, StoreError> {
    let name = text(payload, "name");
    if name.is_empty() {
        return Err(StoreError::Invalid("name is required".into()));
    }
    let savings = money_or_nothing(payload, "estimated_savings")?;
    let cost = money_or_nothing(payload, "implementation_cost")?;
    let id = optional(payload, "id").filter(|given| !given.is_empty());
    let mut body = json!({
        "name": name,
        "description": text(payload, "description"),
        "strategy_type": text(payload, "strategy_type"),
        "estimated_savings": wire(&savings),
        "implementation_cost": wire(&cost),
        "risk_level": vimbai_store::read::or_default(payload, "risk_level", "low"),
        "timeframe": vimbai_store::read::or_default(payload, "timeframe", "short-term"),
    });
    // the shipped model minted one when the caller named none
    body["id"] = match id {
        Some(id) => json!(id),
        None => json!(service_id()),
    };
    Ok(body)
}

pub fn create_strategy<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    remember(graph, scope, TAX_STRATEGY, strategy_in(payload)?)
}

pub fn list_strategies<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
) -> Result<Value, StoreError> {
    let mut found = Records::new(graph, TAX_STRATEGY).list(scope, LISTED)?;
    // newest first, as the shipped service ordered them
    found.sort_by(|left, right| {
        field(right, "created_date")
            .cmp(field(left, "created_date"))
            .then_with(|| field(right, "id").cmp(field(left, "id")))
    });
    Ok(json!({
        "count": found.len(),
        "strategies": found,
    }))
}

/// One strategy's contribution to the plan.
struct Contribution {
    detail: Value,
    recommended: bool,
}

fn contribute(payload: &Value) -> Result<Contribution, StoreError> {
    let savings = money_or_nothing(payload, "estimated_savings")?;
    let cost = money_or_nothing(payload, "implementation_cost")?;
    let net = savings.minus(&cost);
    let risk = vimbai_store::read::or_default(payload, "risk_level", "low");
    // a strategy that costs nothing has no return to state, and one that spends nothing to save something is a
    // division the shipped service refused to make
    let roi = if cost.is_zero() {
        None
    } else {
        Some(net.over(&cost).round(2).to_f64_lossy())
    };
    let recommended =
        matches!(roi, Some(rate) if rate > 1.0) && matches!(risk.as_str(), "low" | "medium");
    let detail = json!({
        "id": field(payload, "id"),
        "name": field(payload, "name"),
        "type": field(payload, "strategy_type"),
        "estimated_savings": wire(&savings),
        "implementation_cost": wire(&cost),
        "net_benefit": wire(&rounded(&net)),
        "risk_level": risk,
        "timeframe": vimbai_store::read::or_default(payload, "timeframe", "short-term"),
        "roi": match roi {
            Some(rate) => json!(rate),
            None => Value::Null,
        },
    });
    Ok(Contribution {
        detail,
        recommended,
    })
}

/// The cent a figure is reported at. Money carries its own scale, so this only trims what is displayed.
fn rounded(amount: &Money) -> Money {
    amount.round(2)
}

pub fn plan(payload: &Value) -> Result<Value, StoreError> {
    let company_id = text(payload, "company_id");
    if company_id.is_empty() {
        return Err(StoreError::Invalid("company_id is required".into()));
    }
    let fiscal_year = whole_signed(payload, "fiscal_year")?;
    let current_tax = money_or_nothing(payload, "current_tax")?;
    let listed = payload
        .get("strategies")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let mut savings = Money::zero();
    let mut costs = Money::zero();
    let mut details: Vec<Value> = Vec::new();
    let mut recommended: Vec<Value> = Vec::new();
    for strategy in &listed {
        let contribution = contribute(strategy)?;
        savings = savings.plus(&money_or_nothing(strategy, "estimated_savings")?);
        costs = costs.plus(&money_or_nothing(strategy, "implementation_cost")?);
        if contribution.recommended {
            recommended.push(json!(field(strategy, "name")));
        }
        details.push(contribution.detail);
    }
    let net = savings.minus(&costs);
    // savings larger than the bill plan a nil bill, never a refund
    let projected = current_tax.minus(&savings);
    let projected = if projected.is_negative() {
        Money::zero()
    } else {
        projected
    };

    Ok(json!({
        "id": service_id(),
        "company_id": company_id,
        "fiscal_year": fiscal_year,
        "current_tax": wire(&rounded(&current_tax)),
        "projected_tax": wire(&rounded(&projected)),
        "total_savings": wire(&rounded(&savings)),
        "net_benefit": wire(&rounded(&net)),
        "strategies": details,
        "recommended_strategies": recommended,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use vimbai_store::fake::FakeGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn strategy(name: &str, savings: f64, cost: f64, risk: &str) -> Value {
        json!({
            "id": format!("s-{name}"),
            "name": name,
            "description": "",
            "strategy_type": "deduction",
            "estimated_savings": savings,
            "implementation_cost": cost,
            "risk_level": risk,
            "timeframe": "short-term",
        })
    }

    #[test]
    fn a_strategy_is_stored_with_the_identifier_it_came_with() {
        let graph = FakeGraph::queue(vec![vec![json!({"id": "s-1"})]]);
        let answer = create_strategy(
            &graph,
            &scope(),
            &strategy("Accelerate deductions", 5000.0, 0.0, "low"),
        )
        .expect("stored");
        assert_eq!(field(&answer, "name"), "Accelerate deductions");
        let (_, params) = graph.last();
        assert_eq!(params["props"]["id"], json!("s-Accelerate deductions"));
    }

    #[test]
    fn a_strategy_named_without_an_identifier_is_given_one() {
        let graph = FakeGraph::queue(vec![vec![json!({"id": "made-1"})]]);
        let mut payload = strategy("Structure the group", 9000.0, 1000.0, "medium");
        payload.as_object_mut().unwrap().remove("id");
        create_strategy(&graph, &scope(), &payload).expect("stored");
        let (_, params) = graph.last();
        assert_eq!(
            params["props"]["id"].as_str().unwrap_or_default().len(),
            36,
            "no identifier was minted: {params}"
        );
    }

    #[test]
    fn the_plan_rounds_its_figures_to_the_cent() {
        let answer = plan(&json!({
            "company_id": "co-1",
            "fiscal_year": 2026,
            "current_taxable_income": 500000,
            "current_tax": 125000.555,
            "strategies": [strategy("Accelerate deductions", 5000.4, 1000.25, "low")],
        }))
        .expect("planned");
        assert_eq!(field(&answer, "current_tax"), "125000.560000");
        assert_eq!(field(&answer, "total_savings"), "5000.400000");
        assert_eq!(field(&answer, "net_benefit"), "4000.150000");
        assert_eq!(field(&answer, "projected_tax"), "120000.160000");
    }

    #[test]
    fn savings_larger_than_the_bill_plan_no_refund() {
        let answer = plan(&json!({
            "company_id": "co-1",
            "fiscal_year": 2026,
            "current_taxable_income": 500000,
            "current_tax": 1000,
            "strategies": [strategy("Treaty relief", 5000.0, 0.0, "low")],
        }))
        .expect("planned");
        assert_eq!(field(&answer, "projected_tax"), "0.000000");
    }

    #[test]
    fn a_strategy_that_costs_nothing_states_no_return_and_is_not_advised() {
        let answer = plan(&json!({
            "company_id": "co-1",
            "fiscal_year": 2026,
            "current_tax": 100000,
            "strategies": [strategy("Free claim", 5000.0, 0.0, "low")],
        }))
        .expect("planned");
        assert_eq!(answer["strategies"][0]["roi"], Value::Null);
        assert_eq!(answer["recommended_strategies"], json!([]));
    }

    #[test]
    fn only_a_low_or_medium_risk_strategy_that_returns_more_than_it_costs_is_advised() {
        let answer = plan(&json!({
            "company_id": "co-1",
            "fiscal_year": 2026,
            "current_tax": 100000,
            "strategies": [
                strategy("Worth doing", 4000.0, 1000.0, "low"),
                strategy("Too risky", 40000.0, 1000.0, "high"),
                strategy("Not worth it", 900.0, 1000.0, "medium"),
                strategy("Break even", 1000.0, 1000.0, "medium"),
            ],
        }))
        .expect("planned");
        assert_eq!(answer["recommended_strategies"], json!(["Worth doing"]));
    }

    #[test]
    fn a_plan_without_a_company_is_refused() {
        assert!(plan(&json!({"fiscal_year": 2026, "current_tax": 1})).is_err());
    }

    #[test]
    fn the_listed_strategies_come_back_newest_first_with_a_count() {
        let graph = FakeGraph::returning(vec![
            json!({"id": "s-1", "name": "Older", "created_date": "2026-01-01T00:00:00"}),
            json!({"id": "s-2", "name": "Newer", "created_date": "2026-02-01T00:00:00"}),
        ]);
        let answer = list_strategies(&graph, &scope()).expect("listed");
        assert_eq!(answer["count"], json!(2));
        assert_eq!(field(&answer["strategies"][0], "id"), "s-2");
    }
}
