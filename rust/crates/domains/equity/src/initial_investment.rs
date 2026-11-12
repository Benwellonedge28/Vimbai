#![forbid(unsafe_code)]
//! initial-investment-service, on the store.
//!
//! What a project costs before it earns anything: the asset, getting it installed, the working capital
//! tied up, other costs, less what the asset is worth at the end.
//!
//! Three shapes are offered. `/create` records the breakdown and keeps it; `/simple` and
//! `/with-capital-allowance` are pure arithmetic and store nothing, so they answer without touching the
//! graph.
//!
//! Kept as shipped:
//!
//! * A line only appears in the breakdown when it is above zero, so an installation cost of zero is
//!   absent rather than nil - but the total still counts it, because adding zero changes nothing.
//! * Scrap value reduces the net investment on `/create` while the total investment keeps it out. The
//!   two figures then answer different questions: what the project ties up gross, and what it ties up
//!   net of recovery.
//! * `/simple` measures the outlay as asset cost less what the old asset raised after tax. Scrap value
//!   is echoed back and never enters the arithmetic, which is what the service did with it.
//! * The capital allowance saving is the allowance times the rate as a percentage, so a rate of 25
//!   means 25 percent and is divided by a hundred.
//!
//! One deliberate change: a missing investment is refused rather than answered 200 with an error body,
//! on this path as on the other record services.

use serde_json::{json, Value};
use vimbai_common::Money;

use vimbai_store::amounts::{amount_or_zero, wire};
use vimbai_store::{BookScope, Graph, Records, StoreError, INITIAL_INVESTMENT};

fn text(payload: &Value, key: &str) -> String {
    payload
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn positive(payload: &Value, key: &str) -> Result<Money, StoreError> {
    amount_or_zero(payload, key)
}

/// The breakdown, with zero lines left out as the service left them out.
pub fn breakdown(items: &[(&str, &str, Money)]) -> Vec<Value> {
    items
        .iter()
        .filter(|(_, _, amount)| *amount > Money::zero())
        .map(|(name, category, amount)| {
            json!({
                "item": name,
                "amount": wire(amount),
                "category": category,
            })
        })
        .collect()
}

/// `/create`.
pub fn create_investment<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let asset = positive(payload, "asset_cost")?;
    let installation = positive(payload, "installation_cost")?;
    let working = positive(payload, "working_capital")?;
    let other = positive(payload, "other_costs")?;
    let scrap = positive(payload, "scrap_value")?;

    let items = breakdown(&[
        ("Asset Cost", "asset", asset.clone()),
        ("Installation Cost", "installation", installation.clone()),
        ("Working Capital", "working_capital", working.clone()),
        ("Other Costs", "other", other.clone()),
    ]);

    let total = asset.plus(&installation).plus(&working).plus(&other);
    let net = total.minus(&scrap);

    Records::new(graph, INITIAL_INVESTMENT).create(
        scope,
        &json!({
            "project_name": text(payload, "project_name"),
            "items": items,
            "total_investment": wire(&total),
            "scrap_value": wire(&scrap),
            "net_investment": wire(&net),
        }),
    )
}

/// `/simple`: asset cost less the after-tax proceeds of the asset it replaces.
pub fn simple_outlay(asset_cost: &Money, sold_for: &Money, tax_on_sale: &Money) -> (Money, Money) {
    let net_proceeds = sold_for.minus(tax_on_sale);
    let outlay = asset_cost.minus(&net_proceeds);
    (net_proceeds, outlay)
}

/// `/with-capital-allowance`: the saving is the allowance at the given percentage.
pub fn capital_allowance_saving(allowance: &Money, rate_percent: &Money) -> Money {
    allowance.times(&rate_percent.over(&Money::from_i64(100)))
}

/// `/list`.
pub fn list_investments<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
) -> Result<Value, StoreError> {
    Ok(json!({
        "investments": Records::new(graph, INITIAL_INVESTMENT).list(scope, 1000)?,
    }))
}

/// `/get/{id}`.
pub fn get_investment<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    investment_id: &str,
) -> Result<Value, StoreError> {
    Records::new(graph, INITIAL_INVESTMENT).get(scope, investment_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::FakeGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn money(text: &str) -> Money {
        Money::parse(text).expect("amount")
    }

    #[test]
    fn zero_lines_stay_out_of_the_breakdown_but_not_out_of_the_total() {
        let graph = FakeGraph::returning(vec![json!({"id": "inv-1"})]);
        create_investment(
            &graph,
            &scope(),
            &json!({"project_name": "Kiln", "asset_cost": "1000", "installation_cost": "0",
                    "working_capital": "250", "scrap_value": "100"}),
        )
        .expect("created");
        let (_, params) = graph.last();
        let written = &params["props"];
        let items = written["items"].as_array().expect("items");
        assert_eq!(items.len(), 2);
        assert_eq!(written["total_investment"], json!("1250.000000"));
        assert_eq!(written["net_investment"], json!("1150.000000"));
    }

    #[test]
    fn every_line_appears_when_every_line_has_a_figure() {
        let graph = FakeGraph::returning(vec![json!({"id": "inv-2"})]);
        create_investment(
            &graph,
            &scope(),
            &json!({"project_name": "Kiln", "asset_cost": "1000", "installation_cost": "50",
                    "working_capital": "250", "other_costs": "0.75"}),
        )
        .expect("created");
        let (_, params) = graph.last();
        assert_eq!(params["props"]["items"].as_array().expect("items").len(), 4);
        assert_eq!(params["props"]["total_investment"], json!("1300.750000"));
    }

    #[test]
    fn a_project_costlier_than_a_double_holds_keeps_every_digit() {
        let graph = FakeGraph::returning(vec![json!({"id": "inv-3"})]);
        create_investment(
            &graph,
            &scope(),
            &json!({"project_name": "Dam", "asset_cost": "123456789012345678901.55",
                    "working_capital": "0.45"}),
        )
        .expect("created");
        let (_, params) = graph.last();
        assert_eq!(
            params["props"]["total_investment"],
            json!("123456789012345678902.000000")
        );
    }

    #[test]
    fn the_simple_outlay_ignores_scrap_value_and_nets_the_tax() {
        let (net, outlay) = simple_outlay(&money("10000"), &money("2500"), &money("500"));
        assert_eq!(wire(&net), "2000.000000");
        assert_eq!(wire(&outlay), "8000.000000");
    }

    #[test]
    fn a_rate_of_twenty_five_means_a_quarter_of_the_allowance() {
        let saving = capital_allowance_saving(&money("8000"), &money("25"));
        assert_eq!(wire(&saving), "2000.000000");
    }

    #[test]
    fn a_missing_investment_is_refused_rather_than_returned_as_an_error_body() {
        let graph = FakeGraph::returning(Vec::new());
        let outcome = get_investment(&graph, &scope(), "nope");
        assert!(matches!(outcome, Err(StoreError::NotFound)));
    }
}
