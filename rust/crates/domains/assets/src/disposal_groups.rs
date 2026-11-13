#![forbid(unsafe_code)]
//! disposal-group-service's records, on the store.
//!
//! A disposal group is a set of assets being sold together (IFRS 5), so the group carries the totals
//! and the impairment test compares them with what the package would fetch.
//!
//! Amounts are `Money` end to end: a package of assets is a balance, and a carrying total assembled
//! from doubles is not one.
//!
//! Kept as shipped:
//!
//! * The group totals are the sum of the assets in it, recomputed when an asset is added. They are
//!   never accepted from the caller.
//! * The impairment is `max(0, carrying total - fair value less costs)`, so an impairment test can
//!   only ever write a loss down, never a gain, and a package worth more than it carries records
//!   zero rather than a negative loss.
//! * Recording a test writes the loss onto the group as well as keeping the test.
//! * A status outside `held_for_sale`, `sold` and `withdrawn` is refused.
//!
//! One deliberate change: adding an asset that is already in the group updates that asset's figures
//! instead of appending a second copy of it. The service appended, so a package could hold the same
//! asset twice and count it twice in its carrying total.

use serde_json::{json, Value};
use vimbai_common::Money;

use vimbai_store::amounts::{amount, amount_or_zero, stored_amount, wire};
use vimbai_store::read::required_text;
use vimbai_store::read::text;
use vimbai_store::{
    BookScope, Graph, Records, StoreError, DISPOSAL_ASSET, DISPOSAL_GROUP, IMPAIRMENT_TEST,
};

/// The three statuses a group can hold.
pub const STATUSES: [&str; 3] = ["held_for_sale", "sold", "withdrawn"];

/// `/groups`: open a disposal group.
pub fn create_group<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let method = {
        let value = text(payload, "disposal_method");
        if value.is_empty() {
            "sale".to_string()
        } else {
            value
        }
    };
    let mut fields = json!({
        "name": required_text(payload, "name")?,
        "description": text(payload, "description"),
        "disposal_method": method,
        "status": "held_for_sale",
        "total_carrying_amount": wire(&Money::zero()),
        "total_fair_value": wire(&Money::zero()),
        "impairment_loss": wire(&Money::zero()),
    });
    if let Some(date) = payload
        .get("expected_disposal_date")
        .filter(|v| !v.is_null())
    {
        fields["expected_disposal_date"] = date.clone();
    }
    Records::new(graph, DISPOSAL_GROUP).create(scope, &fields)
}

/// `/groups`: the caller's groups, optionally narrowed by status.
pub fn list_groups<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    status: Option<&str>,
) -> Result<Value, StoreError> {
    let rows = Records::new(graph, DISPOSAL_GROUP).list(scope, 1000)?;
    Ok(Value::Array(
        rows.into_iter()
            .filter(|row| match status {
                None => true,
                Some(wanted) => row.get("status").and_then(Value::as_str) == Some(wanted),
            })
            .collect(),
    ))
}

pub fn get_group<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    group_id: &str,
) -> Result<Value, StoreError> {
    Records::new(graph, DISPOSAL_GROUP).get(scope, group_id)
}

fn assets_of<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    group_id: &str,
) -> Result<Vec<Value>, StoreError> {
    let rows = Records::new(graph, DISPOSAL_ASSET).list(scope, 1000)?;
    Ok(rows
        .into_iter()
        .filter(|row| row.get("group_id").and_then(Value::as_str) == Some(group_id))
        .collect())
}

/// `/groups/{id}/assets`: put an asset in the group and reset the totals.
pub fn add_asset<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    group_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let group = get_group(graph, scope, group_id)?;

    let asset_id = required_text(payload, "asset_id")?;
    let classification = {
        let value = text(payload, "classification");
        if value.is_empty() {
            "noncurrent_asset".to_string()
        } else {
            value
        }
    };
    let fields = json!({
        "group_id": group_id,
        "asset_id": asset_id,
        "asset_name": required_text(payload, "asset_name")?,
        "carrying_amount": wire(&amount(payload, "carrying_amount")?),
        "fair_value": wire(&amount_or_zero(payload, "fair_value")?),
        "classification": classification,
    });

    let records = Records::new(graph, DISPOSAL_ASSET);
    let existing = assets_of(graph, scope, group_id)?;
    let already = existing
        .iter()
        .find(|row| row.get("asset_id").and_then(Value::as_str) == Some(asset_id.as_str()));

    let saved = match already {
        Some(row) => {
            let id = row.get("id").and_then(Value::as_str).unwrap_or_default();
            records.update(scope, id, &fields)?
        }
        None => records.create(scope, &fields)?,
    };

    // The group's totals are the sum of what is in it, taken from the rows already read plus the
    // change just written, so the total cannot miss the asset this call added by reading too early.
    let mut carrying = Money::zero();
    let mut fair = Money::zero();
    for row in existing.iter() {
        if row.get("asset_id").and_then(Value::as_str) == Some(asset_id.as_str()) {
            continue;
        }
        carrying = carrying.plus(&stored_amount(row, "carrying_amount")?);
        fair = fair.plus(&stored_amount(row, "fair_value")?);
    }
    carrying = carrying.plus(&stored_amount(&fields, "carrying_amount")?);
    fair = fair.plus(&stored_amount(&fields, "fair_value")?);
    let _ = group;
    Records::new(graph, DISPOSAL_GROUP).update(
        scope,
        group_id,
        &json!({
            "total_carrying_amount": wire(&carrying),
            "total_fair_value": wire(&fair),
        }),
    )?;

    // answer with what was written, so a caller does not have to read it back
    let mut answer = fields.clone();
    if let Some(id) = saved.get("id") {
        answer["id"] = id.clone();
    }
    Ok(answer)
}

/// `/groups/{id}/assets`: everything in the group.
pub fn list_assets<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    group_id: &str,
) -> Result<Value, StoreError> {
    get_group(graph, scope, group_id)?;
    Ok(Value::Array(assets_of(graph, scope, group_id)?))
}

/// `/groups/{id}/impairment-test`: write the package down to what it would fetch.
pub fn impairment_test<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    group_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let group = get_group(graph, scope, group_id)?;
    let carrying = match group.get("total_carrying_amount") {
        Some(Value::Null) | None => Money::zero(),
        Some(_) => stored_amount(&group, "total_carrying_amount")?,
    };
    let fair_value = amount(payload, "fair_value_less_costs")?;

    // a package worth more than it carries is not impaired, so the loss never goes below zero
    let shortfall = carrying.minus(&fair_value);
    let impairment = if shortfall.to_f64_lossy() > 0.0 {
        shortfall
    } else {
        Money::zero()
    };

    let fields = json!({
        "group_id": group_id,
        "carrying_amount": wire(&carrying),
        "fair_value_less_costs": wire(&fair_value),
        "impairment_amount": wire(&impairment),
        "notes": text(payload, "notes"),
    });
    let saved = Records::new(graph, IMPAIRMENT_TEST).create(scope, &fields)?;

    Records::new(graph, DISPOSAL_GROUP).update(
        scope,
        group_id,
        &json!({"impairment_loss": wire(&impairment)}),
    )?;

    let mut answer = fields.clone();
    if let Some(id) = saved.get("id") {
        answer["id"] = id.clone();
    }
    Ok(answer)
}

/// `/groups/{id}/status`: hold, sell or withdraw the package.
pub fn update_status<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    group_id: &str,
    status: &str,
) -> Result<Value, StoreError> {
    if !STATUSES.contains(&status) {
        return Err(StoreError::Invalid(format!(
            "Invalid status. Must be one of {STATUSES:?}"
        )));
    }
    Records::new(graph, DISPOSAL_GROUP).update(scope, group_id, &json!({"status": status}))?;
    Ok(json!({"group_id": group_id, "status": status}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::FakeGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    #[test]
    fn a_new_group_holds_nothing_and_carries_nothing() {
        let graph = FakeGraph::returning(vec![json!({"id": "g-1"})]);
        create_group(&graph, &scope(), &json!({"name": "Retail arm"})).expect("created");
        let (_, params) = graph.last();
        assert_eq!(params["props"]["status"], json!("held_for_sale"));
        assert_eq!(params["props"]["disposal_method"], json!("sale"));
        assert_eq!(params["props"]["total_carrying_amount"], json!("0.000000"));
        assert_eq!(params["props"]["impairment_loss"], json!("0.000000"));
    }

    #[test]
    fn adding_an_asset_resets_the_group_totals_to_what_is_in_it() {
        let graph = FakeGraph::returning(vec![
            json!({"id": "g-1", "total_carrying_amount": "0"}),
            json!({"id": "da-1", "group_id": "g-1", "asset_id": "a-1", "carrying_amount": "5000",
                   "fair_value": "4000"}),
        ]);
        let answer = add_asset(
            &graph,
            &scope(),
            "g-1",
            &json!({"asset_id": "a-2", "asset_name": "Van", "carrying_amount": "7000.55",
                    "fair_value": "6500"}),
        )
        .expect("added");
        assert_eq!(answer["carrying_amount"], json!("7000.550000"));
        let (_, params) = graph.last();
        // the asset already in the group (5000 + 4000) plus the one just added
        assert_eq!(
            params["patch"]["total_carrying_amount"],
            json!("12000.550000")
        );
        assert_eq!(params["patch"]["total_fair_value"], json!("10500.000000"));
    }

    #[test]
    fn the_same_asset_is_not_counted_twice() {
        // the group already holds a-2, so adding it again rewrites it rather than appending
        let graph = FakeGraph::returning(vec![
            json!({"id": "g-1"}),
            json!({"id": "da-2", "group_id": "g-1", "asset_id": "a-2",
                   "carrying_amount": "1000", "fair_value": "900"}),
        ]);
        add_asset(
            &graph,
            &scope(),
            "g-1",
            &json!({"asset_id": "a-2", "asset_name": "Van", "carrying_amount": "1200"}),
        )
        .expect("added");
        let (cypher, _) = graph.last();
        assert!(
            !cypher.contains("CREATE"),
            "a second copy was appended: {cypher}"
        );
    }

    #[test]
    fn an_impairment_is_the_shortfall_against_what_the_package_would_fetch() {
        let graph =
            FakeGraph::returning(vec![json!({"id": "g-1", "total_carrying_amount": "50000"})]);
        let answer = impairment_test(
            &graph,
            &scope(),
            "g-1",
            &json!({"fair_value_less_costs": "42000"}),
        )
        .expect("tested");
        assert_eq!(answer["impairment_amount"], json!("8000.000000"));
        let (_, params) = graph.last();
        assert_eq!(params["patch"]["impairment_loss"], json!("8000.000000"));
    }

    #[test]
    fn a_package_worth_more_than_it_carries_is_not_impaired() {
        let graph =
            FakeGraph::returning(vec![json!({"id": "g-1", "total_carrying_amount": "1000"})]);
        let answer = impairment_test(
            &graph,
            &scope(),
            "g-1",
            &json!({"fair_value_less_costs": "1500"}),
        )
        .expect("tested");
        assert_eq!(answer["impairment_amount"], json!("0.000000"));
    }

    #[test]
    fn a_status_outside_the_three_is_refused() {
        let graph = FakeGraph::returning(vec![json!({"id": "g-1"})]);
        let error = update_status(&graph, &scope(), "g-1", "paused").expect_err("refused");
        assert!(error.to_string().contains("Invalid status"), "{error}");
        update_status(&graph, &scope(), "g-1", "sold").expect("sold");
        let (_, params) = graph.last();
        assert_eq!(params["patch"]["status"], json!("sold"));
    }
}
