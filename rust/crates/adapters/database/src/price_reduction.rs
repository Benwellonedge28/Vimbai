//! The price reduction decisions behind selling-price-reduction-service, on the store.
//!
//! A price reduction is judged by what it does to contribution, not to revenue: the service
//! subtracts a variable cost per unit from two prices and multiplies each remainder by an expected
//! volume. Every one of those steps is a subtraction or a multiplication of amounts, so they are done
//! in `Money` and the figures keep their digits: a comparison of two prices that differ by a fraction
//! of a cent is decided by that fraction, and in `f64` the difference could vanish or flip sign.
//!
//! One figure cannot be a number. When the new contribution per unit is not positive, no volume
//! breaks even, and the Python service answered `Infinity` - which is not valid JSON, so any strict
//! client read it as a parse error rather than as "impossible". That case is answered as `null`
//! here, and the recommendation already says what it means (`REJECT`).

#![forbid(unsafe_code)]

use serde_json::{json, Value};
use vimbai_common::Money;

use crate::amounts::{amount, amount_or_zero, stored_amount, wire};
use crate::{BookScope, Graph, Records, StoreError, SELLING_PRICE_ANALYSIS};

/// `Money` states strict positivity as "not zero and not negative", the pair it already offers.
fn positive(money: &Money) -> bool {
    !money.is_zero() && !money.is_negative()
}

fn text<'a>(payload: &'a Value, key: &str) -> Result<&'a str, StoreError> {
    payload
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| StoreError::Invalid(format!("{key} is required")))
}

fn stored_text(row: &Value, key: &str) -> String {
    row.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

/// The percentage an amount change makes of the original, zero when the original is zero.
fn percent(reduction: &Money, original: &Money) -> Money {
    reduction.ratio_pct(original, 6)
}

fn analysis_json(row: &Value) -> Result<Value, StoreError> {
    Ok(json!({
        "id": row.get("id").cloned().unwrap_or(Value::Null),
        "product_id": stored_text(row, "product_id"),
        "product_name": stored_text(row, "product_name"),
        "current_price": wire(&stored_amount(row, "current_price")?),
        "proposed_price": wire(&stored_amount(row, "proposed_price")?),
        "price_reduction": wire(&stored_amount(row, "price_reduction")?),
        "price_reduction_percentage": wire(&stored_amount(row, "price_reduction_percentage")?),
        "current_contribution_per_unit": wire(&stored_amount(row, "current_contribution_per_unit")?),
        "new_contribution_per_unit": wire(&stored_amount(row, "new_contribution_per_unit")?),
        "contribution_change_per_unit": wire(&stored_amount(row, "contribution_change_per_unit")?),
        "current_expected_sales": wire(&stored_amount(row, "current_expected_sales")?),
        "new_expected_sales": wire(&stored_amount(row, "new_expected_sales")?),
        "current_total_contribution": wire(&stored_amount(row, "current_total_contribution")?),
        "new_total_contribution": wire(&stored_amount(row, "new_total_contribution")?),
        "contribution_difference": wire(&stored_amount(row, "contribution_difference")?),
        "minimum_price_for_target_contribution": wire(
            &stored_amount(row, "minimum_price_for_target_contribution")?,
        ),
        "recommendation": stored_text(row, "recommendation"),
        // Null, not a number, when the new contribution per unit is not positive: no volume
        // breaks even, and the Python service's Infinity was not valid JSON.
        "break_even_volume_increase": match row.get("break_even_volume_increase") {
            None | Some(Value::Null) => Value::Null,
            Some(_) => wire(&stored_amount(row, "break_even_volume_increase")?),
        },
        "created_at": row.get("created_at").cloned().unwrap_or(Value::Null),
    }))
}

/// `/analyze`: judge one reduction and store the judgement against the caller's Book.
pub fn create_analysis<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let product_id = text(payload, "product_id")?.to_owned();
    let product_name = text(payload, "product_name")?.to_owned();
    let current_price = amount(payload, "current_price")?;
    let proposed_price = amount(payload, "proposed_price")?;
    let variable_cost = amount(payload, "variable_cost_per_unit")?;
    let current_sales = amount(payload, "current_expected_sales")?;
    let new_sales = match payload.get("new_expected_sales") {
        None | Some(Value::Null) => current_sales.clone(),
        Some(_) => amount(payload, "new_expected_sales")?,
    };
    let target_contribution = amount_or_zero(payload, "target_contribution")?;

    let reduction = current_price.minus(&proposed_price);
    let reduction_pct = percent(&reduction, &current_price);

    let current_unit = current_price.minus(&variable_cost);
    let new_unit = proposed_price.minus(&variable_cost);
    let unit_change = new_unit.minus(&current_unit);

    let current_total = current_unit.times(&current_sales);
    let new_total = new_unit.times(&new_sales);
    let difference = new_total.minus(&current_total);

    // A target contribution quoted for the current volume, expressed back as a price.
    let minimum_price = if target_contribution.is_zero() || current_sales.is_zero() {
        Money::zero()
    } else {
        variable_cost.plus(&target_contribution.over(&current_sales))
    };

    // How much more volume the new price needs to earn what the old price earned. Undefined when the
    // new contribution per unit is not positive, and kept as such rather than as an infinity.
    let break_even = if positive(&new_unit) && !new_sales.is_zero() {
        Some(current_total.over(&new_unit).minus(&current_sales))
    } else {
        None
    };

    let recommendation = if positive(&difference) {
        "APPROVE"
    } else if difference.is_negative() {
        let needs_more_volume = break_even.as_ref().is_some_and(positive);
        if !new_sales.is_zero() && needs_more_volume {
            "REJECT"
        } else {
            "CONDITIONAL_APPROVAL"
        }
    } else {
        "INDIFFERENT"
    };

    let record = json!({
        "product_id": product_id,
        "product_name": product_name,
        "current_price": current_price.to_fixed(6),
        "proposed_price": proposed_price.to_fixed(6),
        "price_reduction": reduction.to_fixed(6),
        "price_reduction_percentage": reduction_pct.to_fixed(6),
        "current_contribution_per_unit": current_unit.to_fixed(6),
        "new_contribution_per_unit": new_unit.to_fixed(6),
        "contribution_change_per_unit": unit_change.to_fixed(6),
        "current_expected_sales": current_sales.to_fixed(6),
        "new_expected_sales": new_sales.to_fixed(6),
        "current_total_contribution": current_total.to_fixed(6),
        "new_total_contribution": new_total.to_fixed(6),
        "contribution_difference": difference.to_fixed(6),
        "minimum_price_for_target_contribution": minimum_price.to_fixed(6),
        "recommendation": recommendation,
        "break_even_volume_increase": break_even.as_ref().map(|be| be.to_fixed(6)),
    });
    let saved = Records::new(graph, SELLING_PRICE_ANALYSIS).create(scope, &record)?;
    analysis_json(&saved)
}

/// `/discount-analysis`: contribution at each discount, held to the same demand response the
/// Python service assumed - each percent off is taken as two percent more volume.
pub fn discount_analysis(payload: &Value) -> Result<Value, StoreError> {
    let product_name = text(payload, "product_name")?.to_owned();
    let list_price = amount(payload, "list_price")?;
    let variable_cost = amount(payload, "variable_cost_per_unit")?;
    let expected_sales = amount(payload, "expected_sales_at_list")?;

    let levels = payload
        .get("discounts")
        .and_then(Value::as_array)
        .ok_or_else(|| StoreError::Invalid("discounts must be a list of percentages".into()))?;

    let hundred = Money::from_i64(100);
    let original_unit = list_price.minus(&variable_cost);
    let original_total = original_unit.times(&expected_sales);

    let mut rows = Vec::with_capacity(levels.len());
    for level in levels {
        let discount = amount(&json!({ "discount": level }), "discount")?;
        // price kept is (100 - discount)/100 of list; volume is (100 + 2*discount)/100 of plan.
        let kept = hundred.minus(&discount).over(&hundred);
        let selling_price = list_price.times(&kept);
        let unit_contribution = selling_price.minus(&variable_cost);
        let volume_multiplier = hundred
            .plus(&discount.times(&Money::from_i64(2)))
            .over(&hundred);
        let estimated_volume = expected_sales.times(&volume_multiplier);
        let total_contribution = unit_contribution.times(&estimated_volume);
        let difference = total_contribution.minus(&original_total);

        rows.push(json!({
            "discount_percentage": wire(&discount),
            "selling_price": wire(&selling_price.round(6)),
            "contribution_per_unit": wire(&unit_contribution.round(6)),
            "estimated_sales_volume": wire(&estimated_volume.round(6)),
            "total_contribution": wire(&total_contribution.round(6)),
            "contribution_vs_original": wire(&difference.round(6)),
            "recommended": positive(&difference),
        }));
    }

    Ok(json!({ "product_name": product_name, "discount_analysis": rows }))
}

/// `/analyses`: the caller's analyses, optionally narrowed to one product.
pub fn analyses<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    product_id: Option<&str>,
) -> Result<Value, StoreError> {
    let wanted = product_id.unwrap_or_default();
    let mut rows = Vec::new();
    for row in Records::new(graph, SELLING_PRICE_ANALYSIS).list(scope, 500)? {
        if !wanted.is_empty() && stored_text(&row, "product_id") != wanted {
            continue;
        }
        rows.push(analysis_json(&row)?);
    }
    Ok(json!({ "analyses": rows }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake::FakeGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn submission() -> Value {
        json!({
            "product_id": "p-1",
            "product_name": "Widget",
            "current_price": 100,
            "proposed_price": 90,
            "variable_cost_per_unit": 60,
            "current_expected_sales": 1000,
            "new_expected_sales": 1500,
        })
    }

    fn stored(record: Value) -> FakeGraph {
        FakeGraph::returning(vec![json!({ "x": record })])
    }

    #[test]
    fn a_reduction_that_earns_more_contribution_is_approved() {
        // 40 a unit on 1000 becomes 30 a unit on 1500: 45000 against 40000.
        let graph = stored(json!({"id": "a-1"}));
        create_analysis(&graph, &scope(), &submission()).unwrap();

        let (_, params) = graph.last();
        let props = &params["props"];
        assert_eq!(props["current_total_contribution"], json!("40000.000000"));
        assert_eq!(props["new_total_contribution"], json!("45000.000000"));
        assert_eq!(props["contribution_difference"], json!("5000.000000"));
        assert_eq!(props["recommendation"], json!("APPROVE"));
        // 40000 at 30 a unit needs 1333.33 units, which is 333 more than the 1000 sold today;
        // the volume is measured against the current plan, as the Python service measured it.
        assert_eq!(props["break_even_volume_increase"], json!("333.333333"));
    }

    #[test]
    fn the_units_of_a_reduction_are_exact() {
        // 100 - 90 is 10 exactly, and 10/100 is ten percent: in f64 both are approximations.
        let graph = stored(json!({"id": "a-1"}));
        create_analysis(&graph, &scope(), &submission()).unwrap();

        let (_, params) = graph.last();
        let props = &params["props"];
        assert_eq!(props["price_reduction"], json!("10.000000"));
        assert_eq!(props["price_reduction_percentage"], json!("10.000000"));
        assert_eq!(props["current_contribution_per_unit"], json!("40.000000"));
        assert_eq!(props["new_contribution_per_unit"], json!("30.000000"));
        assert_eq!(props["contribution_change_per_unit"], json!("-10.000000"));
    }

    #[test]
    fn a_price_below_variable_cost_cannot_break_even_and_says_so_as_null() {
        // Selling under cost: no volume makes the money back. Python answered Infinity, which is
        // not valid JSON; the field is null and the recommendation is what carries the meaning.
        let graph = stored(json!({"id": "a-2"}));
        let payload = json!({
            "product_id": "p-2",
            "product_name": "Dumping",
            "current_price": 100,
            "proposed_price": 50,
            "variable_cost_per_unit": 60,
            "current_expected_sales": 100,
            "new_expected_sales": 0,
        });
        create_analysis(&graph, &scope(), &payload).unwrap();

        let (_, params) = graph.last();
        let props = &params["props"];
        assert_eq!(props["new_contribution_per_unit"], json!("-10.000000"));
        assert_eq!(props["break_even_volume_increase"], Value::Null);
        // no new volume is planned, so the loss is not something more volume could recover
        assert_eq!(props["recommendation"], json!("CONDITIONAL_APPROVAL"));

        // and a submission missing its prices is refused rather than guessed at
        let refused = create_analysis(&graph, &scope(), &Value::Object(Default::default()));
        assert!(matches!(refused, Err(StoreError::Invalid(_))));
    }

    #[test]
    fn a_target_contribution_is_turned_back_into_a_price() {
        // 20000 of contribution over 1000 units is 20 a unit above the 60 cost: 80.
        let mut payload = submission();
        payload["target_contribution"] = json!(20000);
        let graph = stored(json!({"id": "a-3"}));
        create_analysis(&graph, &scope(), &payload).unwrap();
        let (_, params) = graph.last();
        assert_eq!(
            params["props"]["minimum_price_for_target_contribution"],
            json!("80.000000")
        );
    }

    #[test]
    fn a_long_price_keeps_every_digit_while_deciding() {
        // 20 digits in, the difference is decided by the last of them.
        let payload = json!({
            "product_id": "p-3",
            "product_name": "Bulk",
            "current_price": "12345678901234567890.55",
            "proposed_price": "12345678901234567890.54",
            "variable_cost_per_unit": "1",
            "current_expected_sales": "10000000000000000000",
        });
        let graph = stored(json!({"id": "a-4"}));
        create_analysis(&graph, &scope(), &payload).unwrap();
        let (_, params) = graph.last();
        // A float difference would have rounded these to the same number, and the same volume is
        // planned, so the cent comes straight off the contribution.
        assert_eq!(params["props"]["price_reduction"], json!("0.010000"));
        assert_eq!(params["props"]["recommendation"], json!("REJECT"));
    }

    #[test]
    fn a_discount_that_loses_contribution_is_not_recommended() {
        let payload = json!({
            "product_name": "Widget",
            "list_price": 100,
            "variable_cost_per_unit": 40,
            "expected_sales_at_list": 1000,
            "discounts": [0, 10, 50],
        });
        let answer = discount_analysis(&payload).unwrap();
        let rows = answer["discount_analysis"].as_array().unwrap();
        assert_eq!(rows.len(), 3);

        // No discount: list price, plan volume, nothing gained or lost.
        assert_eq!(rows[0]["selling_price"], json!(100.0));
        assert_eq!(rows[0]["contribution_vs_original"], json!(0.0));
        assert_eq!(rows[0]["recommended"], json!(false));

        // Ten percent off keeps 90 of price but adds a fifth more volume: 50 x 1200 beats 60 x 1000.
        assert_eq!(rows[1]["selling_price"], json!(90.0));
        assert_eq!(rows[1]["estimated_sales_volume"], json!(1200.0));
        assert_eq!(rows[1]["contribution_vs_original"], json!(0.0));

        // Half price puts the contribution at 10 a unit, and the volume response cannot rescue it.
        assert_eq!(rows[2]["selling_price"], json!(50.0));
        assert_eq!(rows[2]["recommended"], json!(false));
        assert_eq!(rows[2]["contribution_vs_original"], json!(-40000.0));
    }

    #[test]
    fn analyses_are_filtered_to_one_product_and_one_book() {
        let graph = FakeGraph::returning(vec![
            json!({"x": {"id": "a-1", "product_id": "p-1", "recommendation": "APPROVE"}}),
            json!({"x": {"id": "a-2", "product_id": "p-2", "recommendation": "REJECT"}}),
        ]);
        let all = analyses(&graph, &scope(), None).unwrap();
        assert_eq!(all["analyses"].as_array().unwrap().len(), 2);

        let one = analyses(&graph, &scope(), Some("p-2")).unwrap();
        let rows = one["analyses"].as_array().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["recommendation"], json!("REJECT"));

        // The read was scoped to the caller's Book, not filtered afterwards.
        let (_, params) = graph.last();
        assert_eq!(params["book_id"], json!("book-1"));
    }
}
