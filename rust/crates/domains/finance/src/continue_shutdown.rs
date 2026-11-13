#![forbid(unsafe_code)]
//! continue-shutdown-decision-service: whether a department, product or division should carry on trading or stop.
//!
//! The question is asked in two shapes. `/analyze` takes the whole picture - revenue, variable costs, the fixed costs
//! a shutdown would avoid and those it would not - works out what continuing and what stopping would each leave, and
//! keeps the decision as a record. `/quick-analysis` takes the two figures that actually decide it, the contribution
//! and the unavoidable costs, and gives an answer without keeping anything.
//!
//! Behaviours kept as shipped, each pinned by a test:
//!
//! * contribution is revenue less variable costs;
//! * continuing leaves the contribution less the costs a shutdown cannot avoid;
//! * stopping leaves nothing at all except the costs a shutdown cannot avoid, less what it saves: the avoidable costs
//!   come back, the shutdown costs are paid, and any other saving is added;
//! * the recommendation is whichever leaves more, and indifferent when they are equal;
//! * `/quick-analysis` compares the contribution with the unavoidable costs directly, and says "CONSIDER SHUTDOWN"
//!   rather than "SHUTDOWN" because those two figures alone cannot settle it;
//! * asking for a decision the caller does not hold answers with the shipped `{"error": ...}` body rather than a miss.
//!
//! Not carried: the original carried an HTTP client for accounting-service that no endpoint ever called.

use serde_json::{json, Value};

use vimbai_common::Money;
use vimbai_store::amounts::wire;
use vimbai_store::query::query_page;
use vimbai_store::read::{decimal, field, optional, refused, text};
use vimbai_store::write::remember;
use vimbai_store::{BookScope, Graph, Records, StoreError, CONTINUE_SHUTDOWN_DECISION};

fn amount(payload: &Value, key: &str) -> Result<Money, StoreError> {
    Money::parse(&decimal(payload, key, "0"))
        .map_err(|_| refused(&format!("{key} must be an amount")))
}

/// `POST /analyze`: the whole picture, decided and kept.
pub fn analyze(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let revenue = amount(payload, "revenue")?;
    let variable_costs = amount(payload, "variable_costs")?;
    let avoidable = amount(payload, "avoidable_fixed_costs")?;
    let unavoidable = amount(payload, "unavoidable_fixed_costs")?;
    let shutdown_costs = amount(payload, "shutdown_costs")?;
    let shutdown_savings = amount(payload, "shutdown_savings")?;

    let contribution = revenue.minus(&variable_costs);
    let if_continue = contribution.minus(&unavoidable);
    let net_shutdown_benefit = avoidable.plus(&shutdown_savings).minus(&shutdown_costs);
    let if_shutdown = net_shutdown_benefit.minus(&unavoidable);

    let recommendation = match if_continue.as_decimal().cmp(if_shutdown.as_decimal()) {
        std::cmp::Ordering::Greater => "CONTINUE",
        std::cmp::Ordering::Less => "SHUTDOWN",
        std::cmp::Ordering::Equal => "INDIFFERENT",
    };

    let decision = remember(
        graph,
        scope,
        CONTINUE_SHUTDOWN_DECISION,
        json!({
            "entity_id": text(payload, "entity_id"),
            "entity_name": text(payload, "entity_name"),
            "entity_type": text(payload, "entity_type"),
            "period": text(payload, "period"),
            "cost_analysis": {
                "revenue": wire(&revenue),
                "variable_costs": wire(&variable_costs),
                "contribution": wire(&contribution),
                "avoidable_fixed_costs": wire(&avoidable),
                "unavoidable_fixed_costs": wire(&unavoidable),
            },
            "shutdown_costs": wire(&shutdown_costs),
            "shutdown_savings": wire(&shutdown_savings),
            "net_shutdown_benefit": wire(&net_shutdown_benefit),
            "contribution_if_continue": wire(&contribution),
            "financial_outcome_continue": wire(&if_continue),
            "financial_outcome_shutdown": wire(&if_shutdown),
            "recommendation": recommendation,
        }),
    )?;

    Ok(decision)
}

/// `POST /quick-analysis`: the two figures that settle it, answered without keeping anything.
pub fn quick_decision(payload: &Value) -> Result<Value, StoreError> {
    let name = text(payload, "entity_name");
    let contribution = amount(payload, "contribution")?;
    let unavoidable = amount(payload, "unavoidable_fixed_costs")?;
    let outcome = contribution.minus(&unavoidable);

    let (recommendation, reason) = match contribution.as_decimal().cmp(unavoidable.as_decimal()) {
        std::cmp::Ordering::Greater => (
            "CONTINUE",
            format!(
                "Contribution ({}) exceeds unavoidable costs ({})",
                contribution.to_fixed(2),
                unavoidable.to_fixed(2)
            ),
        ),
        std::cmp::Ordering::Less => (
            "CONSIDER SHUTDOWN",
            format!(
                "Contribution ({}) is less than unavoidable costs ({})",
                contribution.to_fixed(2),
                unavoidable.to_fixed(2)
            ),
        ),
        std::cmp::Ordering::Equal => (
            "INDIFFERENT",
            "Contribution equals unavoidable costs".to_string(),
        ),
    };

    Ok(json!({
        "entity_name": name,
        "contribution": contribution.round(2).to_fixed(2),
        "unavoidable_fixed_costs": unavoidable.round(2).to_fixed(2),
        "financial_outcome": outcome.round(2).to_fixed(2),
        "recommendation": recommendation,
        "reason": reason,
    }))
}

/// `GET /decisions`: the decisions this Book holds, narrowed by what was asked for.
pub fn list(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let limit = query_page(query, 200, 2000);
    let entity_id = optional(query, "entity_id");
    let entity_type = optional(query, "entity_type");

    let mut rows: Vec<Value> = Records::new(graph, CONTINUE_SHUTDOWN_DECISION)
        .list(scope, 2000)?
        .into_iter()
        .filter(|row| {
            entity_id
                .as_deref()
                .map(|wanted| field(row, "entity_id") == wanted)
                .unwrap_or(true)
                && entity_type
                    .as_deref()
                    .map(|wanted| field(row, "entity_type") == wanted)
                    .unwrap_or(true)
        })
        .collect();
    rows.truncate(limit);

    Ok(json!({ "decisions": rows }))
}

/// `GET /decisions/{decision_id}`: the shipped body for a decision the caller does not hold, not a miss.
pub fn get(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    decision_id: &str,
) -> Result<Value, StoreError> {
    let found = Records::new(graph, CONTINUE_SHUTDOWN_DECISION)
        .list(scope, 2000)?
        .into_iter()
        .find(|row| field(row, "id") == decision_id);

    Ok(match found {
        Some(decision) => decision,
        None => json!({ "error": "Decision not found" }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::FakeGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn picture(revenue: &str, variable: &str, avoidable: &str, unavoidable: &str) -> Value {
        json!({
            "entity_id": "d-1", "entity_name": "Milling", "entity_type": "department",
            "period": "2026-Q1", "revenue": revenue, "variable_costs": variable,
            "avoidable_fixed_costs": avoidable, "unavoidable_fixed_costs": unavoidable,
        })
    }

    #[test]
    fn a_department_whose_contribution_covers_its_unavoidable_costs_carries_on() {
        let graph = FakeGraph::queue(vec![vec![json!({"id": "x-1"})]]);
        let answer =
            analyze(&graph, &scope(), &picture("1000", "400", "200", "100")).expect("analysed");
        assert_eq!(answer["contribution_if_continue"], json!("600.000000"));
        assert_eq!(answer["financial_outcome_continue"], json!("500.000000"));
        assert_eq!(answer["recommendation"], json!("CONTINUE"));
    }

    #[test]
    fn a_department_that_loses_money_is_told_to_stop() {
        let graph = FakeGraph::queue(vec![vec![json!({"id": "x-1"})]]);
        let answer =
            analyze(&graph, &scope(), &picture("300", "200", "400", "100")).expect("analysed");
        // continuing leaves 100 - 100 = 0; stopping gives back 400 and keeps the 100 that cannot be avoided
        assert_eq!(answer["financial_outcome_continue"], json!("0.000000"));
        assert_eq!(answer["financial_outcome_shutdown"], json!("300.000000"));
        assert_eq!(answer["recommendation"], json!("SHUTDOWN"));
    }

    #[test]
    fn a_shutdown_cost_reduces_what_stopping_is_worth() {
        let graph = FakeGraph::queue(vec![vec![json!({"id": "x-1"})]]);
        let answer = analyze(
            &graph,
            &scope(),
            &json!({
                "entity_id": "d-1", "entity_name": "Milling", "entity_type": "product",
                "period": "2026-Q1", "revenue": "1000", "variable_costs": "900",
                "avoidable_fixed_costs": "100", "unavoidable_fixed_costs": "50",
                "shutdown_costs": "80", "shutdown_savings": "10",
            }),
        )
        .expect("analysed");
        // stopping: -50 + 100 - 80 + 10 = -20, and continuing: 100 - 50 = 50
        assert_eq!(answer["net_shutdown_benefit"], json!("30.000000"));
        assert_eq!(answer["financial_outcome_shutdown"], json!("-20.000000"));
        assert_eq!(answer["recommendation"], json!("CONTINUE"));
    }

    #[test]
    fn a_picture_that_balances_both_ways_is_indifferent() {
        let graph = FakeGraph::queue(vec![vec![json!({"id": "x-1"})]]);
        let answer =
            analyze(&graph, &scope(), &picture("500", "400", "100", "100")).expect("analysed");
        assert_eq!(answer["financial_outcome_continue"], json!("0.000000"));
        assert_eq!(answer["financial_outcome_shutdown"], json!("0.000000"));
        assert_eq!(answer["recommendation"], json!("INDIFFERENT"));
    }

    #[test]
    fn the_quick_analysis_calls_it_consider_shutdown_rather_than_shutdown() {
        let answer = quick_decision(&json!({
            "entity_name": "Milling", "contribution": "300", "unavoidable_fixed_costs": "400"
        }))
        .expect("analysed");
        assert_eq!(answer["recommendation"], json!("CONSIDER SHUTDOWN"));
        assert_eq!(answer["financial_outcome"], json!("-100.00"));
        assert!(answer["reason"]
            .as_str()
            .unwrap()
            .starts_with("Contribution (300.00)"));
    }

    #[test]
    fn the_quick_analysis_says_continue_when_the_contribution_covers_the_unavoidable() {
        let answer = quick_decision(&json!({
            "entity_name": "Milling", "contribution": "500", "unavoidable_fixed_costs": "400"
        }))
        .expect("analysed");
        assert_eq!(answer["recommendation"], json!("CONTINUE"));
    }

    #[test]
    fn a_decision_the_caller_does_not_hold_answers_with_the_shipped_error_body() {
        let graph = FakeGraph::returning(vec![]);
        let answer = get(&graph, &scope(), "missing").expect("answered");
        assert_eq!(answer["error"], json!("Decision not found"));
    }

    #[test]
    fn the_listing_narrows_by_entity_and_keeps_the_other_books_out() {
        let graph = FakeGraph::returning(vec![
            json!({"id": "x-1", "entity_id": "d-1", "entity_type": "department"}),
            json!({"id": "x-2", "entity_id": "d-2", "entity_type": "product"}),
        ]);
        let answer = list(&graph, &scope(), &json!({"entity_type": "product"})).expect("listed");
        assert_eq!(answer["decisions"].as_array().unwrap().len(), 1);
        assert_eq!(answer["decisions"][0]["id"], json!("x-2"));
    }
}
