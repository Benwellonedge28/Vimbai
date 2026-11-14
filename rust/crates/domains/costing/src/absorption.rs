//! absorption-costing-service's records, on the store.
//!
//! Three of that service's endpoints exist only to hold what it calculated: the product cost
//! list, the latest cost for a product, and the stock valuation that reads it. They are here so
//! the service can answer without a Python process, and so a product cost is one record per Book
//! rather than a row in a dict.

#![forbid(unsafe_code)]

use serde_json::{json, Value};

use vimbai_store::amounts::{amount, amount_or_zero, stored_amount, wire};
use vimbai_store::{BookScope, Graph, Records, StoreError, PRODUCT_COST};

fn text_matches(record: &Value, key: &str, wanted: Option<&str>) -> bool {
    match wanted {
        None => true,
        Some(value) => record.get(key).and_then(Value::as_str) == Some(value),
    }
}

/// `/product-costs`: the caller's product costs, optionally narrowed to one product or period.
pub fn list_product_costs<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    product_id: Option<&str>,
    period: Option<&str>,
) -> Result<Value, StoreError> {
    let rows = Records::new(graph, PRODUCT_COST).list(scope, 500)?;
    let costs: Vec<Value> = rows
        .into_iter()
        .filter(|record| {
            text_matches(record, "product_id", product_id) && text_matches(record, "period", period)
        })
        .collect();
    Ok(json!({ "product_costs": costs }))
}

/// `/product-costs/{product_id}/latest`: the most recent cost recorded for a product.
///
/// The store lists newest first, so the first match is the latest one.
pub fn latest_product_cost<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    product_id: &str,
) -> Result<Value, StoreError> {
    let rows = Records::new(graph, PRODUCT_COST).list(scope, 500)?;
    rows.into_iter()
        .find(|record| text_matches(record, "product_id", Some(product_id)))
        .ok_or(StoreError::NotFound)
}

/// `/stock-valuation`: the latest unit cost times the closing stock the caller recorded.
///
/// The multiplication is exact, so a long closing stock and a long unit cost do not lose digits on
/// the way to the figure.
pub fn stock_valuation<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    product_id: &str,
    valuation_method: &str,
) -> Result<Value, StoreError> {
    let cost = latest_product_cost(graph, scope, product_id)?;
    let unit = stored_amount(&cost, "cost_per_unit")?;
    let closing = stored_amount(&cost, "closing_stock")?;
    Ok(json!({
        "product_id": product_id,
        "valuation_method": valuation_method,
        "cost_per_unit": wire(&unit),
        "closing_stock_units": wire(&closing),
        "closing_stock_value": wire(&unit.times(&closing)),
    }))
}

/// `/product-costs/calculate`: keep the result the calculation produced.
///
/// The payload is the calculation's own output, so the figures are recorded as they were worked
/// out rather than recomputed here.
pub fn record_product_cost<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let product_id = payload
        .get("product_id")
        .and_then(Value::as_str)
        .ok_or_else(|| StoreError::Invalid("product_id is required".into()))?;
    let unit = amount(payload, "cost_per_unit")?;
    let mut record = payload.clone();
    record["cost_per_unit"] = json!(unit.to_fixed(6));
    if record.get("product_id").is_none() {
        record["product_id"] = json!(product_id);
    }
    Records::new(graph, PRODUCT_COST).create(scope, &record)
}

/// `/product-costs/calculate`: what one product took to make this period, and what a unit of it cost.
///
/// The arithmetic is here rather than in a second copy elsewhere: prime cost is the three direct costs added, total
/// production cost adds the overhead, and the unit cost divides one by the other. Quantities are read as exact decimals
/// like every amount, so a million units of a large cost still divides without a float's drift.
pub fn calculate_product_cost(payload: &Value) -> Result<Value, StoreError> {
    let materials = amount_or_zero(payload, "direct_materials")?;
    let labor = amount_or_zero(payload, "direct_labor")?;
    let expenses = amount_or_zero(payload, "direct_expenses")?;
    let overhead = amount_or_zero(payload, "manufacturing_overhead")?;
    let units = amount_or_zero(payload, "units_produced")?;
    let prime_cost = materials.plus(&labor).plus(&expenses);
    let total = prime_cost.plus(&overhead);
    let unit_cost = if units.is_zero() {
        vimbai_common::Money::zero()
    } else {
        total.over(&units).round(6)
    };
    Ok(json!({
        "product_id": option_text(payload, "product_id"),
        "product_name": option_text(payload, "product_name"),
        "period": option_text(payload, "period"),
        "direct_materials": wire(&materials),
        "direct_labor": wire(&labor),
        "direct_expenses": wire(&expenses),
        "manufacturing_overhead": wire(&overhead),
        "units_produced": wire(&units),
        "opening_stock": wire(&amount_or_zero(payload, "opening_stock")?),
        "closing_stock": wire(&amount_or_zero(payload, "closing_stock")?),
        "prime_cost": wire(&prime_cost),
        "total_production_cost": wire(&total),
        "cost_per_unit": wire(&unit_cost),
    }))
}

/// `/overhead/absorption`: the rate overhead is taken up at, and how much of it a period absorbed.
///
/// The rate is the overhead over its base, and what is absorbed is that rate over the actual base, so the two answers
/// cannot disagree: 84000 over 28000 machine hours is 3.00 an hour, and 28000 hours absorb exactly 84000.
pub fn absorb_overhead(payload: &Value) -> Result<Value, StoreError> {
    let overhead_cost = amount_or_zero(payload, "overhead_cost")?;
    let base_units = amount_or_zero(payload, "absorption_base_units")?;
    let actual = match payload.get("actual_base_units") {
        None | Some(Value::Null) => base_units.clone(),
        Some(_) => amount_or_zero(payload, "actual_base_units")?,
    };
    let rate = overhead_cost.over(&base_units).round(6);
    let absorbed = rate.times(&actual).round(2);
    Ok(json!({
        "product_id": option_text(payload, "product_id"),
        "period": option_text(payload, "period"),
        "overhead_cost": wire(&overhead_cost),
        "absorption_base": option_text(payload, "absorption_base"),
        "absorption_base_units": wire(&base_units),
        "actual_base_units": wire(&actual),
        "overhead_absorption_rate": wire(&rate),
        "absorbed_overhead": wire(&absorbed),
        "over_under_absorbed": wire(&absorbed.minus(&overhead_cost)),
    }))
}

/// `/cost-plus`: a selling price that covers the cost and the margin asked for over it.
pub fn cost_plus_selling_price(payload: &Value) -> Result<Value, StoreError> {
    let product_cost = amount_or_zero(payload, "product_cost")?;
    let markup = amount_or_zero(payload, "markup_percentage")?;
    // a markup is a percentage, so twenty means a fifth of the cost, not twenty times it
    let markup_amount = product_cost
        .times(&markup)
        .over(&vimbai_common::Money::from_i64(100))
        .round(2);
    let selling_price = product_cost.plus(&markup_amount).round(2);
    Ok(json!({
        "product_cost": wire(&product_cost),
        "markup_percentage": wire(&markup),
        "markup_amount": wire(&markup_amount),
        "selling_price": wire(&selling_price),
    }))
}

/// A text field a caller may leave out, answered as an empty string rather than refusing the calculation.
fn option_text(payload: &Value, key: &str) -> String {
    payload
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::FakeGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    #[test]
    fn a_product_cost_is_recorded_against_the_callers_book() {
        let graph = FakeGraph::returning(vec![json!({"x": {"id": "pc-1"}})]);
        let saved = record_product_cost(
            &graph,
            &scope(),
            &json!({"product_id": "p-1", "cost_per_unit": 12.5, "closing_stock": 400}),
        );
        // the fake answers the same row to both statements, so trust the shape, not the id
        assert!(saved.is_ok(), "{saved:?}");
        let (cypher, params) = graph.last();
        assert!(cypher.contains("CREATE (x:ProductCost)"), "{cypher}");
        assert_eq!(params["props"]["book_id"], json!("book-1"));
        assert_eq!(params["props"]["cost_per_unit"], json!("12.500000"));
    }

    #[test]
    fn the_latest_cost_is_the_first_one_listed_for_the_product() {
        let graph = FakeGraph::returning(vec![
            json!({"x": {"product_id": "other", "cost_per_unit": "9.00"}}),
            json!({"x": {"product_id": "p-1", "cost_per_unit": "12.50"}}),
        ]);
        let latest = latest_product_cost(&graph, &scope(), "p-1").unwrap();
        assert_eq!(latest["cost_per_unit"], json!("12.50"));
        let missing = latest_product_cost(&graph, &scope(), "p-9");
        assert_eq!(missing.unwrap_err(), StoreError::NotFound);
    }

    #[test]
    fn stock_value_is_the_unit_cost_times_closing_stock_exactly() {
        let graph = FakeGraph::returning(vec![json!({"x": {
            "product_id": "p-1",
            "cost_per_unit": "1234.5678",
            "closing_stock": "3000"
        }})]);
        let out = stock_valuation(&graph, &scope(), "p-1", "fifo").unwrap();
        assert_eq!(out["closing_stock_value"], json!("3703703.400000"));
        assert_eq!(out["valuation_method"], json!("fifo"));
    }

    #[test]
    fn listing_narrows_to_the_product_and_period_asked_for() {
        let graph = FakeGraph::returning(vec![
            json!({"x": {"product_id": "p-1", "period": "2026-Q1"}}),
            json!({"x": {"product_id": "p-1", "period": "2026-Q2"}}),
            json!({"x": {"product_id": "p-2", "period": "2026-Q1"}}),
        ]);
        let out = list_product_costs(&graph, &scope(), Some("p-1"), Some("2026-Q1")).unwrap();
        let costs = out["product_costs"].as_array().unwrap();
        assert_eq!(costs.len(), 1);
        assert_eq!(costs[0]["period"], json!("2026-Q1"));
    }
}
