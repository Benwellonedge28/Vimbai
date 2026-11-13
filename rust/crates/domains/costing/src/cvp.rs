//! The cost-volume-profit arithmetic behind cvp-analysis-service, on the store.
//!
//! Every figure here is one of two things: an amount, or a ratio of two amounts. Both are held in
//! `Money`, because the ratios are what decisions are read off - a contribution margin of exactly a
//! fifth, a margin of safety of exactly a third - and a float remainder would show a business a
//! percentage that its own ledger does not agree with.
//!
//! Three figures have no value when the contribution is not positive: volume never breaks even, no
//! revenue reaches the target. The Python service answered `Infinity` there, which is not valid JSON,
//! so a strict client saw a parse error instead of "impossible". They are answered as `null` here,
//! which is the same decision the price reduction port made.

#![forbid(unsafe_code)]

use serde_json::{json, Value};
use vimbai_common::Money;

use vimbai_store::amounts::{amount, amount_or_zero, stored_amount, wire};
use vimbai_store::read::required;
use vimbai_store::{BookScope, Graph, Records, StoreError, CVP_ANALYSIS};

/// `Money` states strict positivity as "not zero and not negative", the pair it already offers.
fn positive(money: &Money) -> bool {
    !money.is_zero() && !money.is_negative()
}

/// A division that only exists when the divisor is positive; `None` is "no such number".
fn divided_by(numerator: &Money, divisor: &Money) -> Option<Money> {
    if positive(divisor) {
        Some(numerator.over(divisor))
    } else {
        None
    }
}

fn stored_text(row: &Value, key: &str) -> String {
    row.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

/// The stored analysis, read back exactly.
fn analysis_json(row: &Value) -> Result<Value, StoreError> {
    Ok(json!({
        "id": row.get("id").cloned().unwrap_or(Value::Null),
        "entity_id": stored_text(row, "entity_id"),
        "entity_name": stored_text(row, "entity_name"),
        "analysis_date": row.get("analysis_date").cloned().unwrap_or(Value::Null),
        "selling_price_per_unit": wire(&stored_amount(row, "selling_price_per_unit")?),
        "variable_cost_per_unit": wire(&stored_amount(row, "variable_cost_per_unit")?),
        "fixed_costs": wire(&stored_amount(row, "fixed_costs")?),
        "expected_sales_units": wire(&stored_amount(row, "expected_sales_units")?),
        "target_profit": wire(&stored_amount(row, "target_profit")?),
        "contribution_per_unit": wire(&stored_amount(row, "contribution_per_unit")?),
        "contribution_margin_ratio": wire(&stored_amount(row, "contribution_margin_ratio")?),
        "break_even_units": wire(&stored_amount(row, "break_even_units")?),
        "break_even_revenue": wire(&stored_amount(row, "break_even_revenue")?),
        "units_for_target_profit": wire(&stored_amount(row, "units_for_target_profit")?),
        "revenue_for_target_profit": wire(&stored_amount(row, "revenue_for_target_profit")?),
        "expected_sales_revenue": wire(&stored_amount(row, "expected_sales_revenue")?),
        "margin_of_safety_units": wire(&stored_amount(row, "margin_of_safety_units")?),
        "margin_of_safety_revenue": wire(&stored_amount(row, "margin_of_safety_revenue")?),
        "margin_of_safety_percentage": wire(&stored_amount(row, "margin_of_safety_percentage")?),
        "profit_at_expected_sales": wire(&stored_amount(row, "profit_at_expected_sales")?),
        "degree_of_operating_leverage": wire(
            &stored_amount(row, "degree_of_operating_leverage")?,
        ),
        "created_at": row.get("created_at").cloned().unwrap_or(Value::Null),
    }))
}

/// Everything the four stored figures come from, worked out once.
struct Cvp {
    contribution_per_unit: Money,
    margin_ratio: Money,
    break_even_units: Option<Money>,
    break_even_revenue: Option<Money>,
    units_for_target: Option<Money>,
    revenue_for_target: Option<Money>,
    expected_revenue: Money,
    safety_units: Money,
    safety_revenue: Money,
    safety_percentage: Money,
    profit: Money,
    operating_leverage: Option<Money>,
}

fn cvp(
    price: &Money,
    variable_cost: &Money,
    fixed_costs: &Money,
    expected_units: &Money,
    target_profit: &Money,
) -> Cvp {
    let contribution_per_unit = price.minus(variable_cost);
    let margin_ratio = divided_by(&contribution_per_unit, price).unwrap_or_else(Money::zero);

    // Break-even and target profit both stand on a positive contribution: without one, no volume
    // reaches them and the figures are absent rather than infinite.
    let break_even_units = divided_by(fixed_costs, &contribution_per_unit);
    let break_even_revenue = break_even_units.as_ref().map(|units| units.times(price));
    let target = fixed_costs.plus(target_profit);
    let units_for_target = divided_by(&target, &contribution_per_unit);
    let revenue_for_target = units_for_target.as_ref().map(|units| units.times(price));

    let expected_revenue = expected_units.times(price);

    // Margin of safety is only meaningful once something is expected to sell.
    let (safety_units, safety_revenue, safety_percentage) = if positive(expected_units) {
        let units = expected_units.minus(break_even_units.as_ref().unwrap_or(&Money::zero()));
        let revenue = units.times(price);
        let percentage = divided_by(&revenue.times(&Money::from_i64(100)), &expected_revenue)
            .unwrap_or_else(Money::zero);
        (units, revenue, percentage)
    } else {
        (Money::zero(), Money::zero(), Money::zero())
    };

    let profit = expected_units
        .times(&contribution_per_unit)
        .minus(fixed_costs);
    let operating_leverage = if profit.is_zero() {
        None
    } else {
        let contribution_total = expected_units.times(&contribution_per_unit);
        Some(contribution_total.over(&profit))
    };

    Cvp {
        contribution_per_unit,
        margin_ratio,
        break_even_units,
        break_even_revenue,
        units_for_target,
        revenue_for_target,
        expected_revenue,
        safety_units,
        safety_revenue,
        safety_percentage,
        profit,
        operating_leverage,
    }
}

/// `/analyze`: the whole analysis, stored against the caller's Book.
pub fn create_analysis<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let entity_id = required(payload, "entity_id")?.to_owned();
    let entity_name = required(payload, "entity_name")?.to_owned();
    let price = amount(payload, "selling_price_per_unit")?;
    let variable_cost = amount(payload, "variable_cost_per_unit")?;
    let fixed_costs = amount(payload, "fixed_costs")?;
    let expected_units = amount_or_zero(payload, "expected_sales_units")?;
    let target_profit = amount_or_zero(payload, "target_profit")?;

    let figures = cvp(
        &price,
        &variable_cost,
        &fixed_costs,
        &expected_units,
        &target_profit,
    );

    let record = json!({
        "entity_id": entity_id,
        "entity_name": entity_name,
        "analysis_date": payload
            .get("analysis_date")
            .cloned()
            .unwrap_or(Value::Null),
        "selling_price_per_unit": price.to_fixed(6),
        "variable_cost_per_unit": variable_cost.to_fixed(6),
        "fixed_costs": fixed_costs.to_fixed(6),
        "expected_sales_units": expected_units.to_fixed(6),
        "target_profit": target_profit.to_fixed(6),
        "contribution_per_unit": figures.contribution_per_unit.to_fixed(6),
        "contribution_margin_ratio": figures.margin_ratio.to_fixed(6),
        "break_even_units": figures
            .break_even_units
            .as_ref()
            .map(|units| units.to_fixed(6))
            .unwrap_or_else(|| Money::zero().to_fixed(6)),
        "break_even_revenue": figures
            .break_even_revenue
            .as_ref()
            .map(|revenue| revenue.to_fixed(6))
            .unwrap_or_else(|| Money::zero().to_fixed(6)),
        "units_for_target_profit": figures
            .units_for_target
            .as_ref()
            .map(|units| units.to_fixed(6))
            .unwrap_or_else(|| Money::zero().to_fixed(6)),
        "revenue_for_target_profit": figures
            .revenue_for_target
            .as_ref()
            .map(|revenue| revenue.to_fixed(6))
            .unwrap_or_else(|| Money::zero().to_fixed(6)),
        "expected_sales_revenue": figures.expected_revenue.to_fixed(6),
        "margin_of_safety_units": figures.safety_units.to_fixed(6),
        "margin_of_safety_revenue": figures.safety_revenue.to_fixed(6),
        "margin_of_safety_percentage": figures.safety_percentage.to_fixed(6),
        "profit_at_expected_sales": figures.profit.to_fixed(6),
        "degree_of_operating_leverage": figures
            .operating_leverage
            .as_ref()
            .map(|lever| lever.to_fixed(6))
            .unwrap_or_else(|| Money::zero().to_fixed(6)),
    });
    let saved = Records::new(graph, CVP_ANALYSIS).create(scope, &record)?;
    analysis_json(&saved)
}

/// `/quick-analysis`: the four numbers a price list needs, with nothing stored.
pub fn quick_analysis(payload: &Value) -> Result<Value, StoreError> {
    let fixed_costs = amount(payload, "fixed_costs")?;
    let price = amount(payload, "selling_price")?;
    let variable_cost = amount(payload, "variable_cost")?;

    let contribution = price.minus(&variable_cost);
    let ratio = divided_by(&contribution, &price).unwrap_or_else(Money::zero);
    let break_even_units = divided_by(&fixed_costs, &contribution);
    let break_even_revenue = break_even_units.as_ref().map(|units| units.times(&price));

    Ok(json!({
        "fixed_costs": wire(&fixed_costs),
        "selling_price": wire(&price),
        "variable_cost": wire(&variable_cost),
        "contribution_per_unit": wire(&contribution.round(4)),
        "contribution_margin_ratio": wire(&ratio.round(4)),
        // null, not Infinity: without a positive contribution nothing breaks even
        "break_even_units": break_even_units
            .map(|units| wire(&units.round(2)))
            .unwrap_or(Value::Null),
        "break_even_revenue": break_even_revenue
            .map(|revenue| wire(&revenue.round(2)))
            .unwrap_or(Value::Null),
    }))
}

/// `/contribution`: what each unit leaves behind, and what the run of them leaves behind.
pub fn contribution(payload: &Value) -> Result<Value, StoreError> {
    let entity_id = required(payload, "entity_id")?.to_owned();
    let entity_name = required(payload, "entity_name")?.to_owned();
    let price = amount(payload, "selling_price_per_unit")?;
    let variable_cost = amount(payload, "variable_cost_per_unit")?;
    let units_sold = amount_or_zero(payload, "units_sold")?;

    let per_unit = price.minus(&variable_cost);
    let ratio = divided_by(&per_unit, &price).unwrap_or_else(Money::zero);
    let total = per_unit.times(&units_sold);

    Ok(json!({
        "entity_id": entity_id,
        "entity_name": entity_name,
        "selling_price_per_unit": wire(&price),
        "variable_cost_per_unit": wire(&variable_cost),
        "contribution_per_unit": wire(&per_unit.round(4)),
        "contribution_margin_ratio": wire(&ratio.round(4)),
        "contribution_margin_percentage": wire(&ratio.times(&Money::from_i64(100)).round(2)),
        "units_sold": wire(&units_sold),
        "total_contribution": wire(&total.round(2)),
    }))
}

/// `/target-profit`: the volume a profit needs. A contribution that is not positive cannot reach any
/// profit, and the service has always refused that outright rather than answering a number.
pub fn target_profit(payload: &Value) -> Result<Value, StoreError> {
    let entity_name = required(payload, "entity_name")?.to_owned();
    let price = amount(payload, "selling_price_per_unit")?;
    let variable_cost = amount(payload, "variable_cost_per_unit")?;
    let fixed_costs = amount(payload, "fixed_costs")?;
    let target = amount(payload, "target_profit")?;

    let per_unit = price.minus(&variable_cost);
    if !positive(&per_unit) {
        return Err(StoreError::Invalid(
            "contribution per unit must be positive".into(),
        ));
    }
    let ratio = divided_by(&per_unit, &price).unwrap_or_else(Money::zero);
    let units = fixed_costs.plus(&target).over(&per_unit);
    let revenue = units.times(&price);

    let mut answer = json!({
        "entity_name": entity_name,
        "target_profit": wire(&target),
        "contribution_per_unit": wire(&per_unit.round(4)),
        "contribution_margin_ratio": wire(&ratio.round(4)),
        "required_units": wire(&units.round(2)),
        "required_revenue": wire(&revenue.round(2)),
    });

    if let Some(return_on_sales) = payload.get("desired_return_on_sales") {
        if return_on_sales.as_f64().unwrap_or(0.0) > 0.0 {
            let percentage = amount(payload, "desired_return_on_sales")?;
            let revenue_for_ros = target.over(&percentage.over(&Money::from_i64(100)));
            answer["desired_return_on_sales_pct"] = wire(&percentage);
            answer["required_revenue_for_ros"] = wire(&revenue_for_ros.round(2));
        }
    }

    Ok(answer)
}

/// `/multi-product`: break-even for a mix, weighted by how much of the mix each product is.
pub fn multi_product(payload: &Value) -> Result<Value, StoreError> {
    let entity_id = required(payload, "entity_id")?.to_owned();
    let entity_name = required(payload, "entity_name")?.to_owned();
    let fixed_costs = amount(payload, "fixed_costs")?;
    let target = amount_or_zero(payload, "target_profit")?;

    let products = payload
        .get("products")
        .and_then(Value::as_array)
        .ok_or_else(|| StoreError::Invalid("products must be a list".into()))?;
    if products.is_empty() {
        return Err(StoreError::Invalid(
            "at least one product is required".into(),
        ));
    }

    let hundred = Money::from_i64(100);
    let mut weighted = Money::zero();
    let mut details = Vec::with_capacity(products.len());
    let mut weights = Vec::with_capacity(products.len());

    for product in products {
        let name = required(product, "product_name")?.to_owned();
        let price = amount(product, "selling_price")?;
        let variable_cost = amount(product, "variable_cost")?;
        let proportion = amount(product, "expected_proportion")?;

        let per_unit = price.minus(&variable_cost);
        let ratio = divided_by(&per_unit, &price).unwrap_or_else(Money::zero);
        let weight = proportion.over(&hundred);
        weighted = weighted.plus(&ratio.times(&weight));

        weights.push(proportion.clone());
        details.push(json!({
            "product_name": name,
            "selling_price": wire(&price),
            "variable_cost": wire(&variable_cost),
            "contribution_per_unit": wire(&per_unit.round(4)),
            "contribution_margin_ratio": wire(&ratio.round(4)),
            "expected_proportion_pct": wire(&proportion),
        }));
    }

    // A mix whose weighted margin is not positive has no break-even revenue.
    let break_even = divided_by(&fixed_costs, &weighted);
    let target_revenue = divided_by(&fixed_costs.plus(&target), &weighted);

    for (detail, proportion) in details.iter_mut().zip(&weights) {
        let share = proportion.over(&hundred);
        detail["break_even_revenue_share"] = break_even
            .as_ref()
            .map(|revenue| wire(&revenue.times(&share).round(2)))
            .unwrap_or(Value::Null);
        detail["target_revenue_share"] = target_revenue
            .as_ref()
            .map(|revenue| wire(&revenue.times(&share).round(2)))
            .unwrap_or(Value::Null);
    }

    Ok(json!({
        "entity_id": entity_id,
        "entity_name": entity_name,
        "fixed_costs": wire(&fixed_costs),
        "target_profit": wire(&target),
        "weighted_contribution_margin_ratio": wire(&weighted.round(4)),
        "break_even_revenue": break_even
            .map(|revenue| wire(&revenue.round(2)))
            .unwrap_or(Value::Null),
        "target_revenue": target_revenue
            .map(|revenue| wire(&revenue.round(2)))
            .unwrap_or(Value::Null),
        "product_breakdown": details,
    }))
}

/// `/analyses`: the caller's stored analyses, optionally narrowed to one entity.
pub fn analyses<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    entity_id: Option<&str>,
) -> Result<Value, StoreError> {
    let wanted = entity_id.unwrap_or_default();
    let mut rows = Vec::new();
    for row in Records::new(graph, CVP_ANALYSIS).list(scope, 500)? {
        if !wanted.is_empty() && stored_text(&row, "entity_id") != wanted {
            continue;
        }
        rows.push(analysis_json(&row)?);
    }
    Ok(json!({ "total": rows.len(), "analyses": rows }))
}

/// `/analyses/{id}`: one stored analysis, or a 404 that does not say whether it exists elsewhere.
pub fn analysis<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    analysis_id: &str,
) -> Result<Value, StoreError> {
    let row = Records::new(graph, CVP_ANALYSIS)
        .list(scope, 500)?
        .into_iter()
        .find(|row| row.get("id").and_then(Value::as_str) == Some(analysis_id))
        .ok_or(StoreError::NotFound)?;
    analysis_json(&row)
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::FakeGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn props(graph: &FakeGraph) -> Value {
        graph.last().1["props"].clone()
    }

    #[test]
    fn a_break_even_is_units_of_contribution_against_the_fixed_cost() {
        // 100 a unit, 60 variable: 40 of contribution. 8000 of fixed cost needs 200 units.
        let graph = FakeGraph::returning(vec![json!({"x": {"id": "a-1"}})]);
        create_analysis(
            &graph,
            &scope(),
            &json!({
                "entity_id": "e-1",
                "entity_name": "Widget line",
                "selling_price_per_unit": 100,
                "variable_cost_per_unit": 60,
                "fixed_costs": 8000,
                "expected_sales_units": 300,
                "target_profit": 2000,
            }),
        )
        .unwrap();

        let props = props(&graph);
        assert_eq!(props["contribution_per_unit"], json!("40.000000"));
        assert_eq!(props["contribution_margin_ratio"], json!("0.400000"));
        assert_eq!(props["break_even_units"], json!("200.000000"));
        assert_eq!(props["break_even_revenue"], json!("20000.000000"));
        // 8000 + 2000 of profit is 250 units
        assert_eq!(props["units_for_target_profit"], json!("250.000000"));
        assert_eq!(props["revenue_for_target_profit"], json!("25000.000000"));
        // 300 sold against a break-even of 200: 100 units of safety, a third of the plan
        assert_eq!(props["margin_of_safety_units"], json!("100.000000"));
        assert_eq!(props["margin_of_safety_revenue"], json!("10000.000000"));
        assert_eq!(props["margin_of_safety_percentage"], json!("33.333333"));
        // 300 x 40 - 8000 = 4000 of profit, so leverage is 12000/4000
        assert_eq!(props["profit_at_expected_sales"], json!("4000.000000"));
        assert_eq!(props["degree_of_operating_leverage"], json!("3.000000"));
    }

    #[test]
    fn a_price_under_variable_cost_leaves_no_break_even_at_all() {
        // Selling at 50 against a variable cost of 60 loses money on every unit, so no volume ever
        // covers the fixed cost. Python stored an infinity here; the store keeps zero and the
        // endpoint that reports the figure says null.
        let graph = FakeGraph::returning(vec![json!({"x": {"id": "a-2"}})]);
        create_analysis(
            &graph,
            &scope(),
            &json!({
                "entity_id": "e-2",
                "entity_name": "Loss leader",
                "selling_price_per_unit": 50,
                "variable_cost_per_unit": 60,
                "fixed_costs": 8000,
                "expected_sales_units": 300,
            }),
        )
        .unwrap();

        let props = props(&graph);
        assert_eq!(props["contribution_per_unit"], json!("-10.000000"));
        assert_eq!(props["break_even_units"], json!("0.000000"));
        assert_eq!(props["units_for_target_profit"], json!("0.000000"));
        assert_eq!(props["profit_at_expected_sales"], json!("-11000.000000"));

        let quick = quick_analysis(&json!({
            "fixed_costs": 8000,
            "selling_price": 50,
            "variable_cost": 60,
        }))
        .unwrap();
        assert_eq!(quick["break_even_units"], Value::Null);
        assert_eq!(quick["break_even_revenue"], Value::Null);
        assert_eq!(quick["contribution_per_unit"], json!("-10.000000"));
    }

    #[test]
    fn nothing_is_expected_to_sell_so_safety_is_zero_rather_than_a_division() {
        let graph = FakeGraph::returning(vec![json!({"x": {"id": "a-3"}})]);
        create_analysis(
            &graph,
            &scope(),
            &json!({
                "entity_id": "e-3",
                "entity_name": "New line",
                "selling_price_per_unit": 100,
                "variable_cost_per_unit": 60,
                "fixed_costs": 8000,
            }),
        )
        .unwrap();

        let props = props(&graph);
        assert_eq!(props["margin_of_safety_units"], json!("0.000000"));
        assert_eq!(props["margin_of_safety_percentage"], json!("0.000000"));
        // no sales means no profit to lever, so the leverage is zero rather than infinite
        assert_eq!(props["profit_at_expected_sales"], json!("-8000.000000"));
        assert_eq!(props["degree_of_operating_leverage"], json!("0.000000"));
    }

    #[test]
    fn a_target_profit_needs_a_contribution_to_stand_on() {
        let refused = target_profit(&json!({
            "entity_name": "Widget",
            "selling_price_per_unit": 100,
            "variable_cost_per_unit": 100,
            "fixed_costs": 1000,
            "target_profit": 500,
        }));
        assert!(matches!(refused, Err(StoreError::Invalid(_))));

        // 100 - 70 is 30, so 1500 of cost and profit needs 50 units
        let answer = target_profit(&json!({
            "entity_name": "Widget",
            "selling_price_per_unit": 100,
            "variable_cost_per_unit": 70,
            "fixed_costs": 1000,
            "target_profit": 500,
            "desired_return_on_sales": 10,
        }))
        .unwrap();
        assert_eq!(answer["required_units"], json!("50.000000"));
        assert_eq!(answer["required_revenue"], json!("5000.000000"));
        // a 10 percent return on sales turns 500 of profit into 5000 of revenue
        assert_eq!(answer["required_revenue_for_ros"], json!("5000.000000"));
    }

    #[test]
    fn a_mix_is_weighted_by_how_much_of_it_each_product_is() {
        // Half at 50 percent margin and half at 20 percent: the mix earns 35 percent.
        let answer = multi_product(&json!({
            "entity_id": "e-4",
            "entity_name": "Two lines",
            "fixed_costs": 7000,
            "target_profit": 0,
            "products": [
                {"product_name": "A", "selling_price": 100, "variable_cost": 50, "expected_proportion": 50},
                {"product_name": "B", "selling_price": 100, "variable_cost": 80, "expected_proportion": 50}
            ]
        }))
        .unwrap();

        assert_eq!(
            answer["weighted_contribution_margin_ratio"],
            json!("0.350000")
        );
        // 7000 / 0.35 is 20000 of revenue, split evenly
        assert_eq!(answer["break_even_revenue"], json!("20000.000000"));
        let rows = answer["product_breakdown"].as_array().unwrap();
        assert_eq!(rows[0]["break_even_revenue_share"], json!("10000.000000"));
        assert_eq!(rows[1]["contribution_margin_ratio"], json!("0.200000"));
    }

    #[test]
    fn a_mix_that_earns_nothing_has_no_break_even_revenue() {
        let answer = multi_product(&json!({
            "entity_id": "e-5",
            "entity_name": "Cost price",
            "fixed_costs": 7000,
            "products": [
                {"product_name": "A", "selling_price": 100, "variable_cost": 100, "expected_proportion": 100}
            ]
        }))
        .unwrap();
        assert_eq!(
            answer["weighted_contribution_margin_ratio"],
            json!("0.000000")
        );
        assert_eq!(answer["break_even_revenue"], Value::Null);
        assert_eq!(
            answer["product_breakdown"][0]["break_even_revenue_share"],
            Value::Null
        );

        let refused = multi_product(&json!({
            "entity_id": "e-6",
            "entity_name": "Nothing",
            "fixed_costs": 1,
            "products": [],
        }));
        assert!(matches!(refused, Err(StoreError::Invalid(_))));
    }

    #[test]
    fn a_contribution_is_exact_at_the_cent() {
        // A tenth of a cent per unit over ten million units is a thousand of contribution.
        let answer = contribution(&json!({
            "entity_id": "e-7",
            "entity_name": "Volume",
            "selling_price_per_unit": "0.30",
            "variable_cost_per_unit": "0.20",
            "units_sold": "10000000",
        }))
        .unwrap();
        assert_eq!(answer["contribution_per_unit"], json!("0.100000"));
        assert_eq!(answer["total_contribution"], json!("1000000.000000"));
        assert_eq!(answer["contribution_margin_percentage"], json!("33.330000"));
    }

    #[test]
    fn stored_analyses_are_listed_and_fetched_within_the_callers_book() {
        let graph = FakeGraph::returning(vec![
            json!({"x": {"id": "a-1", "entity_id": "e-1", "entity_name": "One"}}),
            json!({"x": {"id": "a-2", "entity_id": "e-2", "entity_name": "Two"}}),
        ]);
        let all = analyses(&graph, &scope(), None).unwrap();
        assert_eq!(all["total"], json!(2));

        let one = analyses(&graph, &scope(), Some("e-2")).unwrap();
        assert_eq!(one["total"], json!(1));
        assert_eq!(one["analyses"][0]["entity_name"], json!("Two"));

        let single = analysis(&graph, &scope(), "a-2").unwrap();
        assert_eq!(single["id"], json!("a-2"));

        let (_, params) = graph.last();
        assert_eq!(params["book_id"], json!("book-1"));
    }

    #[test]
    fn an_analysis_that_is_not_the_callers_is_a_404() {
        let graph = FakeGraph::returning(vec![json!({"x": {"id": "a-1"}})]);
        let answer = analysis(&graph, &scope(), "a-9");
        assert!(matches!(answer, Err(StoreError::NotFound)));
    }
}
