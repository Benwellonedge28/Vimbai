#![forbid(unsafe_code)]
//! scenario-analysis-service: three assumptions set side by side over one base of figures.
//!
//! Each case grows revenue and costs by its own rates, takes depreciation and interest off to reach pre-tax income,
//! charges tax on what is left but only where there is something to charge, and reads the net margin off the revenue.
//! Nothing here is a ledger: `/analyze` is arithmetic over the payload and keeps nothing, while the scenario records
//! below are the assumptions a company wants to remember and compare.
//!
//! Behaviours kept as shipped, each pinned by a test:
//!
//! * an unstated assumption falls back to the shipped rates: revenue up 10%, costs up 5%, interest 5%, tax 25%;
//! * a loss is not taxed, since the charge is on the positive part of pre-tax income only;
//! * the margin is against revenue, and nil rather than a division when there is no revenue;
//! * the revenue sensitivity is the best case's net less the worst case's, and the cost sensitivity is the base case's
//!   net less the worst case's, which is the pair the service reported even though the labels invite a closer reading;
//! * the recommendation follows the worst case first, then the base case, and no other judgement is added;
//! * a comparison needs two scenarios, and reports the range between the best and worst of them.

use crate::figures::{amount, hundred, rate, rounded};
use serde_json::{json, Value};

use vimbai_common::Money;
use vimbai_store::amounts::wire;
use vimbai_store::read::{decimal, field, refused, text};
use vimbai_store::write::remember;
use vimbai_store::{BookScope, Graph, Records, StoreError, SCENARIO};

/// An assumption carried in the payload, where an omitted figure keeps the rate the service shipped.
fn assumption(payload: &Value) -> Value {
    // a rate given as a number or as text counts as stated; anything absent keeps the shipped rate
    let unstated = |key: &str, fallback: &str| -> Money {
        let stated = decimal(payload, key, "");
        if stated.is_empty() {
            Money::parse(fallback).unwrap_or_else(|_| Money::zero())
        } else {
            Money::parse(&stated).unwrap_or_else(|_| Money::zero())
        }
    };
    json!({
        "revenue_growth": wire(&unstated("revenue_growth", "0.1")),
        "cost_growth": wire(&unstated("cost_growth", "0.05")),
        "interest_rate": wire(&unstated("interest_rate", "0.05")),
        "tax_rate": wire(&unstated("tax_rate", "0.25")),
        "capex": wire(&Money::parse(&decimal(payload, "capex", "0")).unwrap_or_else(|_| Money::zero())),
        "description": text(payload, "description"),
    })
}

/// One case worked through from the base figures.
fn case(
    name: &str,
    assumption: &Value,
    revenue: &Money,
    cost: &Money,
    interest: &Money,
    depreciation: &Money,
) -> Value {
    let one = Money::parse("1").unwrap_or_else(|_| Money::zero());
    let projected_revenue = revenue.times(&one.plus(&rate(assumption, "revenue_growth")));
    let projected_cost = cost.times(&one.plus(&rate(assumption, "cost_growth")));
    let ebit = projected_revenue.minus(&projected_cost).minus(depreciation);
    let pretax = ebit.minus(interest);
    // a loss carries no charge
    let taxable = if pretax.is_negative() {
        Money::zero()
    } else {
        pretax.clone()
    };
    let tax = taxable.times(&rate(assumption, "tax_rate"));
    let net = pretax.minus(&tax);
    let margin = if projected_revenue.is_zero() {
        Money::zero()
    } else {
        net.over(&projected_revenue).times(&hundred())
    };

    json!({
        "name": name,
        "description": text(assumption, "description"),
        "projected_revenue": rounded(&projected_revenue),
        "projected_cost": rounded(&projected_cost),
        "ebit": rounded(&ebit),
        "pretax_income": rounded(&pretax),
        "net_income": rounded(&net),
        "net_margin": rounded(&margin),
    })
}

fn net_of(case: &Value) -> Money {
    Money::parse(field(case, "net_income")).unwrap_or_else(|_| Money::zero())
}

/// `POST /analyze`.
pub fn analyze(payload: &Value) -> Result<Value, StoreError> {
    let revenue = amount(payload, "base_revenue", "0")?;
    let cost = amount(payload, "base_cost", "0")?;
    let interest = amount(payload, "base_interest", "0")?;
    let depreciation = amount(payload, "base_depreciation", "0")?;

    let best = case(
        "Best Case",
        &assumption(payload.get("best_case").unwrap_or(&Value::Null)),
        &revenue,
        &cost,
        &interest,
        &depreciation,
    );
    let base = case(
        "Base Case",
        &assumption(payload.get("base_case").unwrap_or(&Value::Null)),
        &revenue,
        &cost,
        &interest,
        &depreciation,
    );
    let worst = case(
        "Worst Case",
        &assumption(payload.get("worst_case").unwrap_or(&Value::Null)),
        &revenue,
        &cost,
        &interest,
        &depreciation,
    );

    let sensitivity_revenue = net_of(&best).minus(&net_of(&worst));
    let sensitivity_cost = net_of(&base).minus(&net_of(&worst));

    // "profitable" means a positive net, so nothing negative and nothing nil
    let recommendation = if !net_of(&worst).is_negative() && !net_of(&worst).is_zero() {
        "All scenarios profitable - proceed with current strategy"
    } else if !net_of(&base).is_negative() && !net_of(&base).is_zero() {
        "Base case profitable but worst case shows losses - implement cost controls"
    } else {
        "Base case unprofitable - immediate restructuring required"
    };

    Ok(json!({
        "id": vimbai_common::service_id(),
        "company_id": text(payload, "company_id"),
        "best_case": best,
        "base_case": base,
        "worst_case": worst,
        "sensitivity_revenue": rounded(&sensitivity_revenue),
        "sensitivity_cost": rounded(&sensitivity_cost),
        "recommendation": recommendation,
    }))
}

/// `POST /scenarios`.
pub fn open_scenario(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let name = text(payload, "name");
    if name.is_empty() {
        return Err(refused("name is required"));
    }
    let net = amount(payload, "net_projection", "0")?;
    remember(
        graph,
        scope,
        SCENARIO,
        json!({
            "company_id": text(payload, "company_id"),
            "name": name,
            "description": text(payload, "description"),
            "assumptions": payload.get("assumptions").cloned().unwrap_or(json!({})),
            "net_projection": wire(&net),
        }),
    )
}

/// `GET /scenarios/{company_id}`.
pub fn scenarios_for_company(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    company_id: &str,
) -> Result<Value, StoreError> {
    let wanted = company_id.to_string();
    Ok(Value::Array(
        Records::new(graph, SCENARIO)
            .list(scope, 2000)?
            .into_iter()
            .filter(|row| field(row, "company_id") == wanted)
            .collect(),
    ))
}

/// `GET /compare/{company_id}`: the best and the worst of what has been kept, and the range between them.
pub fn compare_assumptions(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    company_id: &str,
) -> Result<Value, StoreError> {
    let scenarios: Vec<Value> = Records::new(graph, SCENARIO)
        .list(scope, 2000)?
        .into_iter()
        .filter(|row| field(row, "company_id") == company_id)
        .collect();

    if scenarios.len() < 2 {
        return Ok(json!({
            "comparison": "Need at least 2 scenarios",
            "best_case": "",
            "worst_case": "",
        }));
    }

    let mut best = &scenarios[0];
    let mut worst = &scenarios[0];
    for scenario in &scenarios {
        let net = Money::parse(field(scenario, "net_projection")).unwrap_or_else(|_| Money::zero());
        let best_net =
            Money::parse(field(best, "net_projection")).unwrap_or_else(|_| Money::zero());
        let worst_net =
            Money::parse(field(worst, "net_projection")).unwrap_or_else(|_| Money::zero());
        if net.as_decimal() > best_net.as_decimal() {
            best = scenario;
        }
        if net.as_decimal() < worst_net.as_decimal() {
            worst = scenario;
        }
    }

    let best_net = Money::parse(field(best, "net_projection")).unwrap_or_else(|_| Money::zero());
    let worst_net = Money::parse(field(worst, "net_projection")).unwrap_or_else(|_| Money::zero());

    Ok(json!({
        "best_case": text(best, "name"),
        "worst_case": text(worst, "name"),
        "best_net": wire(&best_net),
        "worst_net": wire(&worst_net),
        "range": wire(&best_net.minus(&worst_net)),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::FakeGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    #[test]
    fn three_cases_are_worked_through_from_one_base() {
        let answer = analyze(&json!({
            "company_id": "c-1", "base_revenue": "1000000", "base_cost": "600000",
            "base_interest": "50000", "base_depreciation": "100000",
            "best_case": {"revenue_growth": 0.2, "cost_growth": 0.0},
            "base_case": {"revenue_growth": 0.1, "cost_growth": 0.05},
            "worst_case": {"revenue_growth": -0.1, "cost_growth": 0.1}
        }))
        .expect("analysed");

        // best: 1,200,000 revenue less 600,000 cost less 100,000 depreciation less 50,000 interest
        assert_eq!(answer["best_case"]["ebit"], json!("500000.00"));
        assert_eq!(answer["best_case"]["net_income"], json!("337500.00"));
        // base: 1,100,000 less 630,000 less 100,000 less 50,000
        assert_eq!(answer["base_case"]["pretax_income"], json!("320000.00"));
        assert_eq!(answer["base_case"]["net_income"], json!("240000.00"));
        // 900,000 less 660,000 less 100,000, less 50,000 interest, taxed at 25%
        assert_eq!(answer["worst_case"]["net_income"], json!("67500.00"));
        assert_eq!(
            answer["recommendation"],
            json!("All scenarios profitable - proceed with current strategy")
        );
    }

    #[test]
    fn an_unstated_assumption_keeps_the_shipped_rates() {
        let answer = analyze(&json!({
            "company_id": "c-1", "base_revenue": "1000", "base_cost": "500",
            "best_case": {}, "base_case": {}, "worst_case": {}
        }))
        .expect("analysed");
        // revenue up 10% to 1,100 and costs up 5% to 525
        assert_eq!(answer["base_case"]["projected_revenue"], json!("1100.00"));
        assert_eq!(answer["base_case"]["projected_cost"], json!("525.00"));
        // 25% of 575 is 143.75
        assert_eq!(answer["base_case"]["net_income"], json!("431.25"));
    }

    #[test]
    fn a_loss_carries_no_tax_charge() {
        let answer = analyze(&json!({
            "company_id": "c-1", "base_revenue": "1000", "base_cost": "900",
            "base_interest": "200",
            "best_case": {}, "base_case": {}, "worst_case": {}
        }))
        .expect("analysed");
        // 1,100 less 945 less 200 leaves a loss, so no charge is added
        assert_eq!(answer["base_case"]["pretax_income"], json!("-45.00"));
        assert_eq!(answer["base_case"]["net_income"], json!("-45.00"));
        assert_eq!(
            answer["recommendation"],
            json!("Base case unprofitable - immediate restructuring required")
        );
    }

    #[test]
    fn the_margin_is_against_revenue_and_nil_when_there_is_none() {
        let answer = analyze(&json!({
            "company_id": "c-1", "base_revenue": "0", "base_cost": "0",
            "best_case": {"revenue_growth": 0}, "base_case": {}, "worst_case": {"revenue_growth": 0}
        }))
        .expect("analysed");
        assert_eq!(answer["best_case"]["net_margin"], json!("0.00"));
    }

    #[test]
    fn a_base_case_that_profits_while_the_worst_does_not_asks_for_cost_control() {
        let answer = analyze(&json!({
            "company_id": "c-1", "base_revenue": "1000", "base_cost": "800",
            "best_case": {"revenue_growth": 0.2},
            "base_case": {"revenue_growth": 0.1, "cost_growth": 0},
            "worst_case": {"revenue_growth": -0.5, "cost_growth": 0.3}
        }))
        .expect("analysed");
        assert!(net_of(&answer["worst_case"]).is_negative());
        assert!(!net_of(&answer["base_case"]).is_negative());
        assert_eq!(
            answer["recommendation"],
            json!("Base case profitable but worst case shows losses - implement cost controls")
        );
    }

    #[test]
    fn a_comparison_of_one_scenario_asks_for_more() {
        let graph = FakeGraph::returning(vec![json!({"id": "s-1", "company_id": "c-1",
                                                     "name": "Base", "net_projection": "100.000000"})]);
        let answer = compare_assumptions(&graph, &scope(), "c-1").expect("compared");
        assert_eq!(answer["comparison"], json!("Need at least 2 scenarios"));
    }

    #[test]
    fn a_comparison_reports_the_best_the_worst_and_the_range() {
        let graph = FakeGraph::returning(vec![
            json!({"id": "s-1", "company_id": "c-1", "name": "Base", "net_projection": "100.000000"}),
            json!({"id": "s-2", "company_id": "c-1", "name": "Best", "net_projection": "250.000000"}),
            json!({"id": "s-3", "company_id": "c-1", "name": "Worst", "net_projection": "40.000000"}),
        ]);
        let answer = compare_assumptions(&graph, &scope(), "c-1").expect("compared");
        assert_eq!(answer["best_case"], json!("Best"));
        assert_eq!(answer["worst_case"], json!("Worst"));
        assert_eq!(answer["range"], json!("210.000000"));
    }

    #[test]
    fn a_scenario_worth_keeping_needs_a_name() {
        let graph = FakeGraph::returning(vec![]);
        assert!(open_scenario(&graph, &scope(), &json!({"company_id": "c-1"})).is_err());
    }
}
