#![forbid(unsafe_code)]
//! fixed-assets-schedule-service: the same register, read as a schedule.
//!
//! This service and the register service were two doors onto one set of `:FixedAsset` nodes - same
//! label, same `:OWNS_FIXED_ASSET` edge, same Book stamp - so each has always answered with the
//! other's records. Rather than port the duplication, both doors now read the one register this
//! crate owns, and this module adds only what the schedule service did that the register did not:
//! a partial update, a dated schedule with its totals, and its own summary shape.
//!
//! The two services spell the same fields differently (`acquisition_cost` and `purchase_cost`,
//! `salvage_value` and `residual_value`, `acquisition_date` and `purchase_date`) because they
//! described the same asset from two directions. A record is written once in one spelling and read
//! under either, so a caller of either service sees the field it asked for.
//!
//! Kept as shipped:
//!
//! * A generated schedule embeds every asset the register holds, at whatever values those records
//!   carry: generating a schedule does not recompute depreciation.
//! * `GET /schedule/latest` regenerates for today and stores another schedule rather than reading
//!   the last one that was stored, so it is an action with a side effect, not a read.
//! * The category summary lists only categories that hold at least one asset.

use std::collections::BTreeMap;

use serde_json::{json, Value};
use vimbai_common::Money;

use vimbai_store::amounts::stored_amount;

use crate::fixed_assets_register::schedule_view;
use vimbai_store::{BookScope, Graph, Records, StoreError, ASSET_SCHEDULE, FIXED_ASSET};

fn text(payload: &Value, key: &str) -> Option<String> {
    payload.get(key).and_then(Value::as_str).map(str::to_string)
}

/// A field under either of the two spellings the two services used.
fn alias<'a>(record: &'a Value, names: &[&str]) -> Option<&'a Value> {
    names.iter().find_map(|name| record.get(*name))
}

fn amount_alias(record: &Value, names: &[&str]) -> Result<Money, StoreError> {
    match alias(record, names) {
        None => Ok(Money::zero()),
        Some(_) => {
            let key = names
                .iter()
                .find(|name| record.get(**name).is_some())
                .copied()
                .unwrap_or(names[0]);
            stored_amount(record, key)
        }
    }
}

fn category_of(record: &Value) -> String {
    text(record, "category").unwrap_or_default()
}

/// `/assets`: the schedule service's envelope, `{"assets": [...], "count": n}`.
pub fn list_with_count<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    category: Option<&str>,
    status: Option<&str>,
) -> Result<Value, StoreError> {
    let rows = Records::new(graph, FIXED_ASSET).list(scope, 1000)?;
    let filtered: Vec<Value> = rows
        .into_iter()
        .map(|row| schedule_view(&row))
        .filter(|row| match category {
            None => true,
            Some(wanted) => category_of(row) == wanted,
        })
        .filter(|row| match status {
            None => true,
            Some(wanted) => text(row, "status").as_deref() == Some(wanted),
        })
        .collect();
    Ok(json!({"assets": filtered, "count": filtered.len()}))
}

/// `PUT /assets/{id}`: merge the supplied fields into the register record.
pub fn update_asset<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    asset_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    if !payload.is_object() {
        return Err(StoreError::Invalid("body must be an object".to_string()));
    }
    // an amount that arrived as a number is rewritten as the string the store keeps
    let mut patch = payload.clone();
    for key in [
        "acquisition_cost",
        "purchase_cost",
        "salvage_value",
        "residual_value",
        "accumulated_depreciation",
        "net_book_value",
    ] {
        if patch.get(key).map(|v| v.is_number()).unwrap_or(false) {
            let exact = stored_amount(&patch, key)?;
            patch[key] = json!(exact.to_fixed(6));
        }
    }
    let updated = Records::new(graph, FIXED_ASSET).update(scope, asset_id, &patch)?;
    Ok(schedule_view(&updated))
}

/// One category bucket of a schedule.
fn bucket(rows: &[&Value]) -> Result<Value, StoreError> {
    let mut cost = Money::zero();
    let mut accumulated = Money::zero();
    let mut nbv = Money::zero();
    for row in rows {
        cost = cost.plus(&amount_alias(row, &["purchase_cost", "acquisition_cost"])?);
        accumulated = accumulated.plus(&amount_alias(row, &["accumulated_depreciation"])?);
        nbv = nbv.plus(&amount_alias(row, &["net_book_value"])?);
    }
    Ok(json!({
        "count": rows.len(),
        "cost": cost.round(2).to_fixed(2),
        "accumulated": accumulated.round(2).to_fixed(2),
        "nbv": nbv.round(2).to_fixed(2),
    }))
}

fn summarise(rows: &[Value]) -> Result<BTreeMap<String, Value>, StoreError> {
    let mut grouped: BTreeMap<String, Vec<&Value>> = BTreeMap::new();
    for row in rows {
        grouped.entry(category_of(row)).or_default().push(row);
    }
    let mut out = BTreeMap::new();
    for (category, bucket_rows) in grouped {
        out.insert(category, bucket(&bucket_rows)?);
    }
    Ok(out)
}

/// `POST /schedule/generate`: totals across the register as at a date, stored as a schedule.
pub fn generate_schedule<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    as_of_date: &str,
) -> Result<Value, StoreError> {
    if as_of_date.trim().is_empty() {
        return Err(StoreError::Invalid("as_of_date is required".to_string()));
    }
    let rows = Records::new(graph, FIXED_ASSET).list(scope, 1000)?;

    let mut total_cost = Money::zero();
    let mut total_accumulated = Money::zero();
    let mut total_nbv = Money::zero();
    for row in &rows {
        total_cost = total_cost.plus(&amount_alias(row, &["purchase_cost", "acquisition_cost"])?);
        total_accumulated =
            total_accumulated.plus(&amount_alias(row, &["accumulated_depreciation"])?);
        total_nbv = total_nbv.plus(&amount_alias(row, &["net_book_value"])?);
    }
    let category_summary = summarise(&rows)?;

    let stored = Records::new(graph, ASSET_SCHEDULE).create(
        scope,
        &json!({
            "as_of_date": as_of_date,
            "asset_count": rows.len(),
            "total_cost": total_cost.round(2).to_fixed(2),
            "total_accumulated_depreciation": total_accumulated.round(2).to_fixed(2),
            "total_net_book_value": total_nbv.round(2).to_fixed(2),
            "category_summary": category_summary,
        }),
    )?;

    Ok(json!({
        "id": stored.get("id").cloned().unwrap_or(Value::Null),
        "as_of_date": as_of_date,
        "assets": rows.iter().map(schedule_view).collect::<Vec<_>>(),
        "total_cost": total_cost.round(2).to_fixed(2),
        "total_accumulated_depreciation": total_accumulated.round(2).to_fixed(2),
        "total_net_book_value": total_nbv.round(2).to_fixed(2),
        "category_summary": category_summary,
    }))
}

/// `/summary`: the schedule service's total shape, over the same register.
pub fn summary<G: Graph + ?Sized>(graph: &G, scope: &BookScope) -> Result<Value, StoreError> {
    let rows = Records::new(graph, FIXED_ASSET).list(scope, 1000)?;
    let mut total_cost = Money::zero();
    let mut total_accumulated = Money::zero();
    let mut total_nbv = Money::zero();
    for row in &rows {
        total_cost = total_cost.plus(&amount_alias(row, &["purchase_cost", "acquisition_cost"])?);
        total_accumulated =
            total_accumulated.plus(&amount_alias(row, &["accumulated_depreciation"])?);
        total_nbv = total_nbv.plus(&amount_alias(row, &["net_book_value"])?);
    }
    Ok(json!({
        "total_assets": rows.len(),
        "total_cost": total_cost.round(2).to_fixed(2),
        "total_accumulated_depreciation": total_accumulated.round(2).to_fixed(2),
        "total_net_book_value": total_nbv.round(2).to_fixed(2),
        "by_category": summarise(&rows)?,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixed_assets_register::create_asset;
    use vimbai_store::fake::FakeGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn register_row(id: &str, category: &str, cost: &str, accumulated: &str, nbv: &str) -> Value {
        json!({
            "id": id,
            "asset_code": format!("C-{id}"),
            "asset_name": format!("Asset {id}"),
            "category": category,
            "purchase_cost": cost,
            "acquisition_cost": cost,
            "accumulated_depreciation": accumulated,
            "net_book_value": nbv,
            "status": "active"
        })
    }

    #[test]
    fn the_list_carries_the_services_envelope() {
        let graph = FakeGraph::returning(vec![
            register_row("a-1", "motor_vehicles", "1000", "200", "800"),
            register_row("a-2", "land", "5000", "0", "5000"),
        ]);
        let listed = list_with_count(&graph, &scope(), None, None).expect("list");
        assert_eq!(listed["count"], json!(2));
        assert_eq!(listed["assets"].as_array().unwrap().len(), 2);

        let vehicles =
            list_with_count(&graph, &scope(), Some("motor_vehicles"), None).expect("list");
        assert_eq!(vehicles["count"], json!(1));
    }

    #[test]
    fn a_schedule_totals_the_register_and_groups_it() {
        let graph = FakeGraph::returning(vec![
            register_row("a-1", "motor_vehicles", "1000", "200", "800"),
            register_row("a-2", "land", "5000", "0", "5000"),
        ]);
        let schedule = generate_schedule(&graph, &scope(), "2026-10-31").expect("schedule");
        assert_eq!(schedule["total_cost"], json!("6000.00"));
        assert_eq!(schedule["total_accumulated_depreciation"], json!("200.00"));
        assert_eq!(schedule["total_net_book_value"], json!("5800.00"));
        assert_eq!(
            schedule["category_summary"]["land"]["nbv"],
            json!("5000.00")
        );
        assert_eq!(
            schedule["category_summary"]["motor_vehicles"]["count"],
            json!(1)
        );
        // the register rows travel with the schedule
        assert_eq!(schedule["assets"].as_array().unwrap().len(), 2);

        // the schedule itself is stored, with the figures as strings
        let (cypher, params) = graph.last();
        assert!(cypher.contains("AssetSchedule"), "{cypher}");
        assert_eq!(params["props"]["total_cost"], json!("6000.00"));
        assert_eq!(params["props"]["asset_count"], json!(2));
    }

    #[test]
    fn the_summary_reads_either_spelling_of_cost() {
        let graph = FakeGraph::returning(vec![json!({
            "id": "a-1", "category": "buildings",
            "purchase_cost": "1500", "accumulated_depreciation": "500", "net_book_value": "1000"
        })]);
        let out = summary(&graph, &scope()).expect("summary");
        assert_eq!(out["total_cost"], json!("1500.00"));
        assert_eq!(out["by_category"]["buildings"]["cost"], json!("1500.00"));
        assert_eq!(
            out["by_category"]["buildings"]["accumulated"],
            json!("500.00")
        );
    }

    #[test]
    fn a_partial_update_is_merged_not_replaced() {
        let graph = FakeGraph::returning(vec![register_row("a-1", "land", "5000", "0", "5000")]);
        update_asset(
            &graph,
            &scope(),
            "a-1",
            &json!({"location": "Bulawayo", "purchase_cost": 6000}),
        )
        .expect("update");
        let (cypher, params) = graph.last();
        assert!(cypher.contains("SET x += $patch"), "{cypher}");
        assert_eq!(params["patch"]["location"], json!("Bulawayo"));
        // the amount arrives as an exact string, never as a double
        assert_eq!(params["patch"]["purchase_cost"], json!("6000.000000"));
    }

    #[test]
    fn an_empty_body_is_refused() {
        let graph = FakeGraph::returning(vec![]);
        let error = update_asset(&graph, &scope(), "a-1", &json!("nope")).expect_err("refused");
        assert!(error.to_string().contains("object"), "{error}");
    }

    #[test]
    fn a_record_is_stored_once_and_read_under_either_spelling() {
        let graph = FakeGraph::returning(vec![json!({
            "id": "a-1", "category": "plant_machinery",
            "acquisition_cost": "1000", "acquisition_date": "2024-01-01", "salvage_value": "100",
            "accumulated_depreciation": "300", "net_book_value": "700"
        })]);
        let listed = list_with_count(&graph, &scope(), None, None).expect("list");
        let asset = &listed["assets"][0];
        assert_eq!(asset["purchase_cost"], json!("1000"));
        assert_eq!(asset["purchase_date"], json!("2024-01-01"));
        assert_eq!(asset["residual_value"], json!("100"));
        // the stored figure is never duplicated: one cost, two names at the edge
        assert_eq!(asset["acquisition_cost"], json!("1000"));
    }

    #[test]
    fn a_schedule_shaped_payload_registers_into_the_one_register() {
        let graph = FakeGraph::returning(vec![json!({"id": "a-1"})]);
        create_asset(
            &graph,
            &scope(),
            &json!({
                "asset_code": "PM-1", "asset_name": "Press", "category": "plant_machinery",
                "purchase_date": "2023-06-01", "purchase_cost": "50000",
                "useful_life_years": 10, "residual_value": "5000"
            }),
        )
        .expect("registered");
        let (_, params) = graph.last();
        let props = &params["props"];
        assert_eq!(props["acquisition_cost"], json!("50000.000000"));
        assert_eq!(props["acquisition_date"], json!("2023-06-01"));
        assert_eq!(props["salvage_value"], json!("5000.000000"));
        assert_eq!(props["net_book_value"], json!("50000.000000"));
        assert!(props.get("purchase_cost").is_none());
    }
}
