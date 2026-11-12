#![forbid(unsafe_code)]
//! fixed-assets-register-service's records, on the store.
//!
//! An asset carries its cost, its accumulated depreciation and its net book value, and the two
//! writes that matter - depreciating an asset and disposing of it - have to move the asset and
//! record the entry in one place, which is why this module is more than a wrapper around the store.
//!
//! The figures are `Money`: the service held floats, so a register with a long cost drifted in the
//! last digits as depreciation accumulated. Stored amounts are exact and leave as decimal strings.
//!
//! Kept as shipped:
//!
//! * A category must be one of the service's seven, and `IT` really is upper case.
//! * Straight line charges `(cost - salvage) / (life * 12)` a month, and a zero life charges
//!   nothing rather than dividing.
//! * Reducing balance charges 20% of the CURRENT net book value a year, twelve times over, so it
//!   compounds monthly and never lands exactly on the salvage value.
//! * Any other depreciation method charges nothing per period.
//! * An asset that depreciates down to its salvage value has its net book value pinned there and
//!   its status set to `disposed` - not to a "fully depreciated" state, which is what the status
//!   vocabulary suggests but not what the service did.
//! * Disposing an asset records `proceeds - net book value` as the gain or loss and leaves the
//!   accumulated depreciation where it is.
//!
//! The register and the schedule service write the same `:FixedAsset` nodes by the same edge, so
//! both services have always been reading and writing one register between them. One set of record
//! kinds serves both here, because splitting them would invent a distinction the data never made.

use std::collections::BTreeMap;

use serde_json::{json, Value};
use vimbai_common::Money;

use vimbai_store::amounts::{amount, stored_amount, wire};
use vimbai_store::{
    BookScope, Graph, Records, StoreError, ASSET_DISPOSAL, DEPRECIATION_ENTRY, FIXED_ASSET,
};

/// The seven categories the service accepted. `IT` is upper case on the wire.
pub const CATEGORIES: [&str; 7] = [
    "land",
    "buildings",
    "vehicles",
    "machinery",
    "furniture",
    "equipment",
    "IT",
];

fn text(payload: &Value, key: &str) -> String {
    payload
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn required_text(payload: &Value, key: &str) -> Result<String, StoreError> {
    let value = text(payload, key);
    if value.trim().is_empty() {
        return Err(StoreError::Invalid(format!("{key} is required")));
    }
    Ok(value)
}

fn whole(payload: &Value, key: &str) -> Result<i64, StoreError> {
    match payload.get(key) {
        Some(Value::Number(number)) => number
            .as_i64()
            .ok_or_else(|| StoreError::Invalid(format!("{key} must be a whole number"))),
        Some(Value::String(text)) => text
            .trim()
            .parse::<i64>()
            .map_err(|_| StoreError::Invalid(format!("{key} must be a whole number"))),
        _ => Err(StoreError::Invalid(format!("{key} is required"))),
    }
}

/// `/assets`: register an asset, stamped with the caller's Book.
pub fn create_asset<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let category = required_text(payload, "category")?;
    if !CATEGORIES.contains(&category.as_str()) {
        return Err(StoreError::Invalid(format!(
            "Invalid category. Must be one of {CATEGORIES:?}"
        )));
    }

    let cost = amount(payload, "acquisition_cost")?;
    let salvage = match payload.get("salvage_value") {
        Some(Value::Null) | None => Money::zero(),
        Some(_) => amount(payload, "salvage_value")?,
    };
    let method = {
        let value = text(payload, "depreciation_method");
        if value.is_empty() {
            "straight_line".to_string()
        } else {
            value
        }
    };

    let mut fields = json!({
        "asset_code": required_text(payload, "asset_code")?,
        "asset_name": required_text(payload, "asset_name")?,
        "category": category,
        "location": text(payload, "location"),
        "department": text(payload, "department"),
        "acquisition_date": required_text(payload, "acquisition_date")?,
        "acquisition_cost": wire(&cost),
        "useful_life_years": whole(payload, "useful_life_years")?,
        "salvage_value": wire(&salvage),
        "depreciation_method": method,
        "accumulated_depreciation": wire(&Money::zero()),
        // the service registered every asset at cost, never at cost less salvage
        "net_book_value": wire(&cost),
        "status": "active",
    });
    if let Some(notes) = payload.get("notes").and_then(Value::as_str) {
        fields["notes"] = json!(notes);
    }

    Records::new(graph, FIXED_ASSET).create(scope, &fields)
}

/// `/assets`: the caller's register, optionally narrowed.
pub fn list_assets<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    category: Option<&str>,
    status: Option<&str>,
    department: Option<&str>,
) -> Result<Value, StoreError> {
    let rows = Records::new(graph, FIXED_ASSET).list(scope, 1000)?;
    let keep = |row: &Value, key: &str, wanted: Option<&str>| match wanted {
        None => true,
        Some(value) => row.get(key).and_then(Value::as_str) == Some(value),
    };
    let filtered: Vec<Value> = rows
        .into_iter()
        .filter(|row| keep(row, "category", category))
        .filter(|row| keep(row, "status", status))
        .filter(|row| keep(row, "department", department))
        .collect();
    Ok(Value::Array(filtered))
}

pub fn get_asset<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    asset_id: &str,
) -> Result<Value, StoreError> {
    Records::new(graph, FIXED_ASSET).get(scope, asset_id)
}

/// The monthly charge for one asset, by the method the register recorded.
fn monthly_charge(asset: &Value) -> Result<Money, StoreError> {
    let method = asset
        .get("depreciation_method")
        .and_then(Value::as_str)
        .unwrap_or("straight_line");
    let cost = stored_amount(asset, "acquisition_cost")?;
    let salvage = stored_amount(asset, "salvage_value")?;
    let net_book_value = stored_amount(asset, "net_book_value")?;
    let life = asset
        .get("useful_life_years")
        .and_then(Value::as_i64)
        .unwrap_or(0);

    Ok(match method {
        "straight_line" => {
            if life <= 0 {
                Money::zero()
            } else {
                cost.minus(&salvage)
                    .over(&Money::from_i64(life * 12))
                    .round(2)
            }
        }
        "reducing_balance" => {
            // 20% a year on the current net book value, charged monthly
            net_book_value
                .times(&Money::from_i64(20))
                .over(&Money::from_i64(1200))
                .round(2)
        }
        _ => Money::zero(),
    })
}

/// `/assets/{id}/depreciate`: charge one period and record the entry.
pub fn depreciate<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    asset_id: &str,
    period: &str,
) -> Result<Value, StoreError> {
    if period.trim().is_empty() {
        return Err(StoreError::Invalid("period is required".to_string()));
    }
    let assets = Records::new(graph, FIXED_ASSET);
    let asset = assets.get(scope, asset_id)?;

    let status = asset
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("active");
    if status != "active" {
        return Err(StoreError::Invalid(format!("Asset is {status}")));
    }

    let cost = stored_amount(&asset, "acquisition_cost")?;
    let salvage = stored_amount(&asset, "salvage_value")?;
    let charge = monthly_charge(&asset)?;

    let accumulated = stored_amount(&asset, "accumulated_depreciation")?.plus(&charge);
    let mut net_book_value = cost.minus(&accumulated);
    let mut new_status = "active";
    if net_book_value.to_f64_lossy() <= salvage.to_f64_lossy() {
        // the service pinned the value at salvage and called the asset disposed
        net_book_value = salvage.clone();
        new_status = "disposed";
    }

    assets.update(
        scope,
        asset_id,
        &json!({
            "accumulated_depreciation": wire(&accumulated),
            "net_book_value": wire(&net_book_value),
            "status": new_status,
        }),
    )?;

    Records::new(graph, DEPRECIATION_ENTRY).create(
        scope,
        &json!({
            "asset_id": asset_id,
            "period": period,
            "depreciation_amount": wire(&charge),
            "accumulated_depreciation": wire(&accumulated),
            "net_book_value": wire(&net_book_value),
            "method": asset
                .get("depreciation_method")
                .and_then(Value::as_str)
                .unwrap_or("straight_line"),
        }),
    )
}

/// `/assets/{id}/depreciation`: the entries recorded against one asset.
pub fn list_entries<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    asset_id: &str,
) -> Result<Value, StoreError> {
    let rows = Records::new(graph, DEPRECIATION_ENTRY).list(scope, 1000)?;
    Ok(Value::Array(
        rows.into_iter()
            .filter(|row| row.get("asset_id").and_then(Value::as_str) == Some(asset_id))
            .collect(),
    ))
}

/// `/assets/{id}/dispose`: record the disposal and mark the asset disposed.
pub fn dispose<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    asset_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let assets = Records::new(graph, FIXED_ASSET);
    let asset = assets.get(scope, asset_id)?;
    if asset.get("status").and_then(Value::as_str) == Some("disposed") {
        return Err(StoreError::Invalid("Asset already disposed".to_string()));
    }

    let proceeds = match payload.get("disposal_value") {
        Some(Value::Null) | None => Money::zero(),
        Some(_) => amount(payload, "disposal_value")?,
    };
    let net_book_value = stored_amount(&asset, "net_book_value")?;
    let gain_loss = proceeds.minus(&net_book_value);

    let method = {
        let value = text(payload, "disposal_method");
        if value.is_empty() {
            "sale".to_string()
        } else {
            value
        }
    };

    assets.update(
        scope,
        asset_id,
        &json!({
            "status": "disposed",
            "accumulated_depreciation": wire(&stored_amount(&asset, "accumulated_depreciation")?),
            "net_book_value": wire(&net_book_value),
        }),
    )?;

    Records::new(graph, ASSET_DISPOSAL).create(
        scope,
        &json!({
            "asset_id": asset_id,
            "disposal_date": required_text(payload, "disposal_date")?,
            "disposal_value": wire(&proceeds),
            "disposal_method": method,
            "gain_loss": wire(&gain_loss),
            "notes": text(payload, "notes"),
        }),
    )
}

/// `/summary`: the register totals, in exact amounts.
pub fn summary<G: Graph + ?Sized>(graph: &G, scope: &BookScope) -> Result<Value, StoreError> {
    let rows = Records::new(graph, FIXED_ASSET).list(scope, 1000)?;

    let mut total_cost = Money::zero();
    let mut total_accumulated = Money::zero();
    let mut total_net_book_value = Money::zero();
    let mut active = 0usize;
    let mut by_category: BTreeMap<String, (usize, Money)> = BTreeMap::new();

    for row in &rows {
        let cost = stored_amount(row, "acquisition_cost")?;
        let accumulated = stored_amount(row, "accumulated_depreciation")?;
        let net_book_value = stored_amount(row, "net_book_value")?;
        total_cost = total_cost.plus(&cost);
        total_accumulated = total_accumulated.plus(&accumulated);
        total_net_book_value = total_net_book_value.plus(&net_book_value);
        if row.get("status").and_then(Value::as_str) == Some("active") {
            active += 1;
        }
        let category = row
            .get("category")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let entry = by_category.entry(category).or_insert((0, Money::zero()));
        entry.0 += 1;
        entry.1 = entry.1.plus(&net_book_value);
    }

    let categories: BTreeMap<String, Value> = by_category
        .into_iter()
        .map(|(category, (count, net_book_value))| {
            (
                category,
                json!({"count": count, "nbv": net_book_value.round(2).to_fixed(2)}),
            )
        })
        .collect();

    Ok(json!({
        "total_assets": rows.len(),
        "active_assets": active,
        "total_acquisition_cost": total_cost.round(2).to_fixed(2),
        "total_accumulated_depreciation": total_accumulated.round(2).to_fixed(2),
        "total_net_book_value": total_net_book_value.round(2).to_fixed(2),
        "by_category": categories,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::FakeGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn asset_payload() -> Value {
        json!({
            "asset_code": "MV-001",
            "asset_name": "Delivery van",
            "category": "vehicles",
            "acquisition_date": "2024-01-01",
            "acquisition_cost": "240000",
            "useful_life_years": 5,
            "salvage_value": "40000",
            "depreciation_method": "straight_line",
            "location": "Harare",
            "department": "Logistics"
        })
    }

    fn stored_asset(fields: Value) -> Value {
        json!({
            "id": "a-1",
            "asset_code": "MV-001",
            "asset_name": "Delivery van",
            "category": "vehicles",
            "department": "Logistics",
            "acquisition_date": "2024-01-01",
            "acquisition_cost": "240000",
            "useful_life_years": 5,
            "salvage_value": "40000",
            "depreciation_method": "straight_line",
            "accumulated_depreciation": "0",
            "net_book_value": "240000",
            "status": "active",
            "created_at": "2026-11-01T00:00:00Z"
        })
        .as_object()
        .unwrap()
        .clone()
        .into_iter()
        .map(|(k, v)| match fields.get(&k) {
            Some(override_value) => (k, override_value.clone()),
            None => (k, v),
        })
        .collect::<serde_json::Map<String, Value>>()
        .into()
    }

    #[test]
    fn the_seven_categories_are_enforced_and_it_is_upper_case() {
        let graph = FakeGraph::returning(vec![]);
        let error = create_asset(
            &graph,
            &scope(),
            &json!({
                "asset_code": "MV-1", "asset_name": "Van", "category": "vehicles2",
                "acquisition_date": "2024-01-01", "acquisition_cost": "1000", "useful_life_years": 5
            }),
        )
        .expect_err("refused");
        assert!(error.to_string().contains("Invalid category"), "{error}");

        let ok_graph = FakeGraph::returning(vec![stored_asset(json!({}))]);
        let ok = create_asset(
            &ok_graph,
            &scope(),
            &json!({
                "asset_code": "IT-1", "asset_name": "Laptop", "category": "IT",
                "acquisition_date": "2024-01-01", "acquisition_cost": "1000", "useful_life_years": 3
            }),
        );
        assert!(ok.is_ok(), "{:?}", ok.err());
    }

    #[test]
    fn a_registered_asset_starts_at_cost_with_no_depreciation() {
        let graph = FakeGraph::returning(vec![stored_asset(json!({}))]);
        let created = create_asset(&graph, &scope(), &asset_payload()).expect("created");
        let (_, params) = graph.last();
        let props = &params["props"];
        assert_eq!(props["net_book_value"], json!("240000.000000"));
        assert_eq!(props["accumulated_depreciation"], json!("0.000000"));
        assert_eq!(props["status"], json!("active"));
        // a register write never carries the scope in the payload
        assert_eq!(props["book_id"], json!("book-1"));
        assert_eq!(props["user_id"], json!("user-1"));
        assert!(created.get("id").is_some());
    }

    #[test]
    fn straight_line_charges_cost_less_salvage_over_the_months() {
        let asset = stored_asset(json!({}));
        // (240000 - 40000) / (5 * 12)
        assert_eq!(monthly_charge(&asset).unwrap().to_fixed(2), "3333.33");
    }

    #[test]
    fn a_zero_life_charges_nothing_rather_than_dividing() {
        let asset = stored_asset(json!({"useful_life_years": 0}));
        assert_eq!(monthly_charge(&asset).unwrap(), Money::zero());
    }

    #[test]
    fn reducing_balance_charges_twenty_percent_a_year_on_the_current_value() {
        let asset = stored_asset(json!({
            "depreciation_method": "reducing_balance",
            "net_book_value": "120000"
        }));
        // 120000 * 20% / 12
        assert_eq!(monthly_charge(&asset).unwrap().to_fixed(2), "2000.00");
    }

    #[test]
    fn an_unknown_method_charges_nothing() {
        let asset = stored_asset(json!({"depreciation_method": "units_of_production"}));
        assert_eq!(monthly_charge(&asset).unwrap(), Money::zero());
    }

    #[test]
    fn depreciating_moves_the_asset_and_records_the_entry() {
        let graph = FakeGraph::returning(vec![
            stored_asset(json!({})),
            stored_asset(
                json!({"accumulated_depreciation": "3333.33", "net_book_value": "236666.67"}),
            ),
            json!({"id": "d-1", "asset_id": "a-1", "period": "2026-10",
                   "depreciation_amount": "3333.33", "accumulated_depreciation": "3333.33",
                   "net_book_value": "236666.67", "method": "straight_line"}),
        ]);
        depreciate(&graph, &scope(), "a-1", "2026-10").expect("entry");
        let (cypher, params) = graph.last();
        assert!(cypher.contains("DepreciationEntry"), "{cypher}");
        assert_eq!(params["props"]["depreciation_amount"], json!("3333.330000"));
        assert_eq!(
            params["props"]["accumulated_depreciation"],
            json!("3333.330000")
        );
        assert_eq!(params["props"]["net_book_value"], json!("236666.670000"));
        assert_eq!(params["props"]["period"], json!("2026-10"));
        assert_eq!(params["props"]["method"], json!("straight_line"));
    }

    #[test]
    fn an_asset_at_salvage_is_pinned_there_and_called_disposed() {
        let graph = FakeGraph::returning(vec![
            stored_asset(json!({
                "accumulated_depreciation": "197000",
                "net_book_value": "43000"
            })),
            stored_asset(json!({"status": "disposed"})),
            json!({"id": "d-2"}),
        ]);
        depreciate(&graph, &scope(), "a-1", "2026-11").expect("entry");
        let (cypher, params) = graph.last();
        assert!(cypher.contains("DepreciationEntry"), "{cypher}");
        // 43000 - 3333.33 would fall below the 40000 salvage, so the value is pinned there
        assert_eq!(params["props"]["net_book_value"], json!("40000.000000"));
    }

    #[test]
    fn a_disposed_asset_is_not_depreciated_again() {
        let graph = FakeGraph::returning(vec![stored_asset(json!({"status": "disposed"}))]);
        let error = depreciate(&graph, &scope(), "a-1", "2026-11").expect_err("refused");
        assert_eq!(error.to_string(), "Asset is disposed");
    }

    #[test]
    fn disposal_records_the_difference_between_proceeds_and_book_value() {
        let graph = FakeGraph::returning(vec![
            stored_asset(json!({"net_book_value": "100000", "accumulated_depreciation": "140000"})),
            stored_asset(json!({"status": "disposed"})),
            json!({"id": "x-1", "asset_id": "a-1", "gain_loss": "20000.00"}),
        ]);
        let disposal = dispose(
            &graph,
            &scope(),
            "a-1",
            &json!({"disposal_date": "2026-10-31", "disposal_value": "120000",
                    "disposal_method": "sale", "notes": "sold to driver"}),
        )
        .expect("disposal");
        assert!(disposal.is_object());

        // the disposal records the difference, and the asset keeps its accumulated depreciation
        let (cypher, params) = graph.last();
        assert!(cypher.contains("AssetDisposal"), "{cypher}");
        assert_eq!(params["props"]["gain_loss"], json!("20000.000000"));
        assert_eq!(params["props"]["disposal_method"], json!("sale"));
        assert_eq!(params["props"]["notes"], json!("sold to driver"));
    }

    #[test]
    fn a_loss_making_disposal_is_negative_and_an_already_disposed_asset_is_refused() {
        let graph = FakeGraph::returning(vec![
            stored_asset(json!({"net_book_value": "100000"})),
            stored_asset(json!({"status": "disposed"})),
            json!({"id": "x-2"}),
        ]);
        let disposal = dispose(
            &graph,
            &scope(),
            "a-1",
            &json!({"disposal_date": "2026-10-31", "disposal_value": "80000"}),
        )
        .expect("disposal");
        assert!(disposal.is_object());
        let (cypher, params) = graph.last();
        assert!(cypher.contains("AssetDisposal"), "{cypher}");
        assert_eq!(params["props"]["gain_loss"], json!("-20000.000000"));
        assert_eq!(params["props"]["disposal_method"], json!("sale"));

        let refused = FakeGraph::returning(vec![stored_asset(json!({"status": "disposed"}))]);
        let error = dispose(
            &refused,
            &scope(),
            "a-1",
            &json!({"disposal_date": "2026-10-31", "disposal_value": "1"}),
        )
        .expect_err("refused");
        assert_eq!(error.to_string(), "Asset already disposed");
    }

    #[test]
    fn the_summary_totals_exactly_and_groups_by_category() {
        let graph = FakeGraph::returning(vec![
            stored_asset(json!({"category": "vehicles"})),
            stored_asset(json!({
                "id": "a-2",
                "category": "IT",
                "acquisition_cost": "10000",
                "accumulated_depreciation": "4000",
                "net_book_value": "6000"
            })),
            stored_asset(json!({
                "id": "a-3",
                "category": "vehicles",
                "status": "disposed",
                "net_book_value": "0"
            })),
        ]);
        let summary = summary(&graph, &scope()).expect("summary");
        assert_eq!(summary["total_assets"], json!(3));
        assert_eq!(summary["active_assets"], json!(2));
        assert_eq!(summary["total_acquisition_cost"], json!("490000.00"));
        assert_eq!(summary["total_net_book_value"], json!("246000.00"));
        assert_eq!(summary["by_category"]["vehicles"]["count"], json!(2));
        assert_eq!(
            summary["by_category"]["vehicles"]["nbv"],
            json!("240000.00")
        );
        assert_eq!(summary["by_category"]["IT"]["nbv"], json!("6000.00"));
    }

    #[test]
    fn amounts_past_double_precision_keep_every_digit() {
        let graph = FakeGraph::returning(vec![stored_asset(json!({
            "acquisition_cost": "123456789012345678901.55",
            "net_book_value": "123456789012345678901.55",
            "salvage_value": "0",
            "useful_life_years": 100
        }))]);
        let charge = monthly_charge(&stored_asset(json!({
            "acquisition_cost": "123456789012345678901.55",
            "net_book_value": "123456789012345678901.55",
            "salvage_value": "0",
            "useful_life_years": 100
        })))
        .unwrap();
        // 1200 months of service, so the charge is the cost over 1200
        assert_eq!(charge.to_fixed(2), "102880657510288065.75");
        assert_eq!(graph.statements(), 0);
    }
}
