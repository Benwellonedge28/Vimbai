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
//! * An asset is written off by one of three methods, chosen per asset when it is registered and
//!   recorded on it: `straight_line`, `reducing_balance` and `time_based`. Any other method is
//!   refused in words rather than charging nothing, which is what it used to do: a register that
//!   quietly takes nothing off an asset reads exactly like one where the asset never wears out.
//! * Straight line charges `(cost - salvage) / (life * 12)` a month, and a zero life charges
//!   nothing rather than dividing.
//! * Reducing balance charges a share of the CURRENT net book value a year, twelve times over, so
//!   it compounds monthly. The share is the asset's own rate when it states one, and otherwise the
//!   life's own share (`100 / life` years) - the shipped 20% was only right for a five-year asset,
//!   and a van with a ten-year life written down at 20% a year loses more than it was ever worth.
//! * Time based charges for the time the asset has actually been held inside the period: the
//!   annual charge spread over 365 days and taken for the days the asset was yours, so a van bought
//!   on the 20th is charged for the part of that month it was in the yard, and the last period of
//!   its life is a part-period too.
//! * No method charges more than what is left to write off. The last charge of an asset's life is
//!   the remainder, not a whole month of it, so accumulated depreciation and net book value on the
//!   register can never disagree about the same asset.
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
use vimbai_common::{dates, Money};

use vimbai_store::amounts::{amount, stored_amount, wire};
use vimbai_store::read::{key_under, required_text};
use vimbai_store::read::{text, whole_signed};
use vimbai_store::{
    BookScope, Graph, Records, StoreError, ASSET_DISPOSAL, DEPRECIATION_ENTRY, FIXED_ASSET,
};

/// The categories the register service accepted. `IT` is upper case on the wire.
pub const CATEGORIES: [&str; 7] = [
    "land",
    "buildings",
    "vehicles",
    "machinery",
    "furniture",
    "equipment",
    "IT",
];

/// The ways an asset's cost can be written off, one chosen per asset.
///
/// The register records the choice on the asset, so a business can write a van down by reducing balance and a
/// building by straight line, which is what the two rules are for.
pub const METHODS: [&str; 3] = ["straight_line", "reducing_balance", "time_based"];

/// The categories the schedule service accepted.
///
/// The two services named the same shelves differently (`machinery` and `plant_machinery`,
/// `furniture` and `furniture_fixtures`, `vehicles` and `motor_vehicles`), and both wrote into the
/// same register, so a category is valid if either service knew it. Refusing one vocabulary would
/// refuse records the platform already holds.
pub const SCHEDULE_CATEGORIES: [&str; 9] = [
    "land",
    "buildings",
    "plant_machinery",
    "furniture_fixtures",
    "motor_vehicles",
    "office_equipment",
    "computer_equipment",
    "leasehold_improvements",
    "intangible_assets",
];

fn known_category(category: &str) -> bool {
    CATEGORIES.contains(&category) || SCHEDULE_CATEGORIES.contains(&category)
}

/// The same record under the schedule service's field names.
///
/// The two services spelled the same figures differently. A record is stored once, under the
/// register's names, and this projects the second spelling at the edge, so neither service's
/// clients have to change and no figure is stored twice.
pub fn schedule_view(record: &Value) -> Value {
    let mut view = record.clone();
    let mirrors = [
        ("acquisition_cost", "purchase_cost"),
        ("acquisition_date", "purchase_date"),
        ("salvage_value", "residual_value"),
    ];
    if let Some(object) = view.as_object_mut() {
        for (canonical, alias) in mirrors {
            if let Some(value) = object.get(canonical).cloned() {
                object.entry(alias.to_string()).or_insert(value);
            }
        }
    }
    view
}

/// `/assets`: register an asset, stamped with the caller's Book.
pub fn create_asset<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let category = required_text(payload, "category")?;
    if !known_category(&category) {
        return Err(StoreError::Invalid(format!(
            "Invalid category. Must be one of {CATEGORIES:?}"
        )));
    }

    // an amount under either spelling the two services used
    let cost = amount(
        payload,
        key_under(payload, &["acquisition_cost", "purchase_cost"])
            .ok_or_else(|| StoreError::Invalid("acquisition_cost is required".to_string()))?,
    )?;
    let salvage = match key_under(payload, &["salvage_value", "residual_value"]) {
        None => Money::zero(),
        Some(key) => amount(payload, key)?,
    };
    let method = {
        let value = text(payload, "depreciation_method");
        if value.is_empty() {
            "straight_line".to_string()
        } else {
            value
        }
    };
    if !METHODS.contains(&method.as_str()) {
        return Err(StoreError::Invalid(format!(
            "Unknown depreciation method {method}. An asset is written off by one of {METHODS:?}"
        )));
    }
    // only a reducing-balance asset reads this, but a rate of nothing or of more than the whole asset a year is a
    // mistake wherever it is written down, and the person is told now rather than when the charge comes out wrong
    let rate = stored_amount(payload, "depreciation_rate")?;
    if rate.to_f64_lossy() < 0.0 || rate.to_f64_lossy() > 100.0 {
        return Err(StoreError::Invalid(format!(
            "depreciation_rate must be a percentage of the asset a year, between 0 and 100; got {}",
            rate.to_fixed(2)
        )));
    }

    let mut fields = json!({
        "asset_code": required_text(payload, "asset_code")?,
        "asset_name": required_text(payload, "asset_name")?,
        "category": category,
        "location": text(payload, "location"),
        "department": text(payload, "department"),
        "acquisition_date": required_text(
            payload,
            key_under(payload, &["acquisition_date", "purchase_date"])
                .ok_or_else(|| StoreError::Invalid("acquisition_date is required".to_string()))?,
        )?,
        "acquisition_cost": wire(&cost),
        "useful_life_years": whole_signed(payload, "useful_life_years")?,
        "salvage_value": wire(&salvage),
        "depreciation_method": method,
        "accumulated_depreciation": wire(&Money::zero()),
        // the service registered every asset at cost, never at cost less salvage
        "net_book_value": wire(&cost),
        "status": "active",
    });
    if rate.to_f64_lossy() > 0.0 {
        fields["depreciation_rate"] = wire(&rate);
    }
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

/// The method the register recorded for an asset, refusing one the register does not know.
fn method_of(asset: &Value) -> Result<&str, StoreError> {
    let method = asset
        .get("depreciation_method")
        .and_then(Value::as_str)
        .unwrap_or("straight_line");
    if !METHODS.contains(&method) {
        return Err(StoreError::Invalid(format!(
            "Unknown depreciation method {method} on this asset. The register writes an asset off by one of \
             {METHODS:?}"
        )));
    }
    Ok(method)
}

/// The share of what is left a reducing-balance asset gives up in a year, as a percentage.
///
/// The rate the asset states wins, because a business writing a machine down faster than its life says so has said
/// so. With no stated rate the share is the life's own, so a five-year asset is still charged the shipped 20% and a
/// ten-year one is charged 10% rather than wearing out in five.
fn annual_percent(asset: &Value, life: i64) -> Money {
    let stated = stored_amount(asset, "depreciation_rate").unwrap_or_else(|_| Money::zero());
    if stated.to_f64_lossy() > 0.0 {
        stated
    } else if life > 0 {
        Money::from_i64(100).over(&Money::from_i64(life))
    } else {
        Money::zero()
    }
}

/// The first and last day of a `YYYY-MM` period, counted in days from 1970-01-01.
fn period_days(period: &str) -> Option<(i64, i64)> {
    let text = period.trim();
    let mut parts = text.split('-');
    let year: i64 = parts.next()?.parse().ok()?;
    let month: i64 = parts.next()?.parse().ok()?;
    if parts.next().is_some() || !(1..=12).contains(&month) {
        return None;
    }
    let start = dates::day_of(&format!("{year:04}-{month:02}-01"))?;
    let (next_year, next_month) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    let after = dates::day_of(&format!("{next_year:04}-{next_month:02}-01"))?;
    Some((start, after - 1))
}

/// What a time-based asset charges in one period: the annual charge spread over the days it was held.
///
/// A period before the asset arrived, or one that opens after its life has run out, is refused in words rather than
/// answering nothing: a charge of nothing in a ledger column looks like a month the asset stood still, and a person
/// reading it cannot tell the difference between that and a mistake.
fn time_based_charge(
    asset: &Value,
    period: &str,
    cost: &Money,
    salvage: &Money,
    life: i64,
) -> Result<Money, StoreError> {
    if life <= 0 {
        return Err(StoreError::Invalid(format!(
            "{period} cannot be charged by time: the asset has no useful life, so there is no rate to charge at"
        )));
    }
    let (start, end) = period_days(period).ok_or_else(|| {
        StoreError::Invalid(format!(
            "{period} is not a month written YYYY-MM, so the days it holds cannot be counted"
        ))
    })?;
    let acquired_on = asset
        .get("acquisition_date")
        .and_then(Value::as_str)
        .unwrap_or("");
    let acquired = dates::day_of(acquired_on).ok_or_else(|| {
        StoreError::Invalid(format!(
            "The asset's acquisition date {acquired_on:?} is not a date, so the time it has been held cannot be counted"
        ))
    })?;
    if end < acquired {
        return Err(StoreError::Invalid(format!(
            "{period} is before the asset was acquired on {}, so there is nothing to charge",
            dates::date_of(acquired)
        )));
    }
    // the life runs out on the anniversary of the day it arrived, counted in days
    let life_ends = acquired + life * 365;
    let from = start.max(acquired);
    if from > life_ends {
        return Err(StoreError::Invalid(format!(
            "{period} is after the asset's {life}-year life ended on {}",
            dates::date_of(life_ends)
        )));
    }
    // only the days inside both the period and the life are charged, so the first and last periods are part-periods
    let days = end.min(life_ends) - from + 1;
    let annual = cost.minus(salvage).over(&Money::from_i64(life));
    Ok(annual
        .times(&Money::from_i64(days))
        .over(&Money::from_i64(365))
        .round(2))
}

/// What one asset charges in one period, by the method the register recorded for it.
///
/// The charge never takes an asset below what it is worth at the end: the last period of its life is charged the
/// remainder, so `accumulated_depreciation` and `net_book_value` on the register keep adding up to the cost.
fn charge_for(asset: &Value, period: &str) -> Result<Money, StoreError> {
    let method = method_of(asset)?;
    let cost = stored_amount(asset, "acquisition_cost")?;
    let salvage = stored_amount(asset, "salvage_value")?;
    let accumulated = stored_amount(asset, "accumulated_depreciation")?;
    let net_book_value = stored_amount(asset, "net_book_value")?;
    let life = asset
        .get("useful_life_years")
        .and_then(Value::as_i64)
        .unwrap_or(0);

    let charge = match method {
        "straight_line" => {
            if life <= 0 {
                Money::zero()
            } else {
                cost.minus(&salvage)
                    .over(&Money::from_i64(life * 12))
                    .round(2)
            }
        }
        "reducing_balance" => net_book_value
            .times(&annual_percent(asset, life))
            .over(&Money::from_i64(1200))
            .round(2),
        "time_based" => time_based_charge(asset, period, &cost, &salvage, life)?,
        other => {
            return Err(StoreError::Invalid(format!(
                "Unknown depreciation method {other} on this asset"
            )))
        }
    };

    let remaining = cost.minus(&salvage).minus(&accumulated);
    if charge.to_f64_lossy() > remaining.to_f64_lossy() {
        Ok(if remaining.to_f64_lossy() > 0.0 {
            remaining
        } else {
            Money::zero()
        })
    } else {
        Ok(charge)
    }
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

    // a month is charged once. Charging it again would write the asset down twice, and the register alone would not show
    // it: two rows for one month look like two months.
    let already = Records::new(graph, DEPRECIATION_ENTRY).list(scope, 1000)?;
    if already.iter().any(|row| {
        row.get("asset_id").and_then(Value::as_str) == Some(asset_id)
            && row.get("period").and_then(Value::as_str) == Some(period.trim())
    }) {
        return Err(StoreError::Invalid(format!(
            "Depreciation already recorded for period {}",
            period.trim()
        )));
    }

    let cost = stored_amount(&asset, "acquisition_cost")?;
    let salvage = stored_amount(&asset, "salvage_value")?;
    let charge = charge_for(&asset, period.trim())?;

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
    let mut disposed = 0usize;
    let mut by_category: BTreeMap<String, (usize, Money)> = BTreeMap::new();

    for row in &rows {
        let held = row.get("status").and_then(Value::as_str) == Some("active");
        if held {
            active += 1;
        } else {
            disposed += 1;
        }
        // a disposed asset keeps its row and its last book value, and neither is something the business still owns: the
        // totals below are what is held, so a sold van cannot stay in what the register is worth
        if !held {
            continue;
        }
        let cost = stored_amount(row, "acquisition_cost")?;
        let accumulated = stored_amount(row, "accumulated_depreciation")?;
        let net_book_value = stored_amount(row, "net_book_value")?;
        total_cost = total_cost.plus(&cost);
        total_accumulated = total_accumulated.plus(&accumulated);
        total_net_book_value = total_net_book_value.plus(&net_book_value);
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
        "disposed_assets": disposed,
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
            "depreciation_rate": "0",
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
        assert_eq!(
            charge_for(&asset, "2026-11").unwrap().to_fixed(2),
            "3333.33"
        );
    }

    #[test]
    fn a_zero_life_charges_nothing_rather_than_dividing() {
        let asset = stored_asset(json!({"useful_life_years": 0}));
        assert_eq!(charge_for(&asset, "2026-11").unwrap(), Money::zero());
    }

    #[test]
    fn reducing_balance_charges_twenty_percent_a_year_on_the_current_value() {
        let asset = stored_asset(json!({
            "depreciation_method": "reducing_balance",
            "net_book_value": "120000"
        }));
        // 120000 * 20% / 12
        assert_eq!(
            charge_for(&asset, "2026-11").unwrap().to_fixed(2),
            "2000.00"
        );
    }

    #[test]
    fn an_unknown_method_is_refused_rather_than_charging_nothing() {
        // a register that quietly takes nothing off an asset reads exactly like one where it never wears out
        let asset = stored_asset(json!({"depreciation_method": "units_of_production"}));
        let error = charge_for(&asset, "2026-11").expect_err("refused");
        assert!(error.to_string().contains("units_of_production"), "{error}");
        assert!(error.to_string().contains("time_based"), "{error}");
    }

    #[test]
    fn a_method_the_register_does_not_know_is_refused_when_the_asset_is_registered() {
        let graph = FakeGraph::returning(vec![stored_asset(json!({}))]);
        let mut payload = asset_payload();
        payload["depreciation_method"] = json!("units_of_production");
        let error = create_asset(&graph, &scope(), &payload).expect_err("refused");
        assert!(
            error.to_string().contains("Unknown depreciation method"),
            "{error}"
        );
        // nothing was written
        assert_eq!(graph.statements(), 0);
    }

    #[test]
    fn each_of_the_three_methods_can_be_chosen_for_an_asset() {
        for method in METHODS {
            let graph = FakeGraph::returning(vec![stored_asset(json!({}))]);
            let mut payload = asset_payload();
            payload["depreciation_method"] = json!(method);
            create_asset(&graph, &scope(), &payload).expect(method);
            let (_, params) = graph.last();
            assert_eq!(params["props"]["depreciation_method"], json!(method));
        }
    }

    #[test]
    fn a_reducing_balance_rate_beyond_the_whole_asset_a_year_is_refused() {
        let graph = FakeGraph::returning(vec![stored_asset(json!({}))]);
        let mut payload = asset_payload();
        payload["depreciation_method"] = json!("reducing_balance");
        payload["depreciation_rate"] = json!("120");
        let error = create_asset(&graph, &scope(), &payload).expect_err("refused");
        assert!(error.to_string().contains("between 0 and 100"), "{error}");
    }

    #[test]
    fn reducing_balance_charges_the_life_its_own_share_not_a_fixed_twentieth() {
        // a ten-year asset used to be written down at 20% a year, wearing out in five
        let asset = stored_asset(json!({
            "depreciation_method": "reducing_balance",
            "useful_life_years": 10,
            "net_book_value": "120000"
        }));
        // 120000 * 10% / 12
        assert_eq!(
            charge_for(&asset, "2026-11").unwrap().to_fixed(2),
            "1000.00"
        );
    }

    #[test]
    fn reducing_balance_takes_the_rate_the_asset_states() {
        let asset = stored_asset(json!({
            "depreciation_method": "reducing_balance",
            "depreciation_rate": "25",
            "net_book_value": "120000"
        }));
        // 120000 * 25% / 12
        assert_eq!(
            charge_for(&asset, "2026-11").unwrap().to_fixed(2),
            "2500.00"
        );
    }

    #[test]
    fn time_based_charges_a_whole_month_when_the_asset_was_held_all_of_it() {
        let asset = stored_asset(json!({
            "depreciation_method": "time_based",
            "acquisition_date": "2024-01-01"
        }));
        // (240000 - 40000) / 5 years = 40000 a year; November has 30 days: 40000 * 30 / 365
        assert_eq!(
            charge_for(&asset, "2026-11").unwrap().to_fixed(2),
            "3287.67"
        );
    }

    #[test]
    fn time_based_charges_only_the_days_the_asset_was_held_in_its_first_month() {
        // bought on the 20th, so eleven days of that month were the business's: 40000 * 11 / 365
        let asset = stored_asset(json!({
            "depreciation_method": "time_based",
            "acquisition_date": "2026-11-20"
        }));
        assert_eq!(
            charge_for(&asset, "2026-11").unwrap().to_fixed(2),
            "1205.48"
        );
    }

    #[test]
    fn time_based_refuses_a_month_before_the_asset_arrived() {
        let asset = stored_asset(json!({
            "depreciation_method": "time_based",
            "acquisition_date": "2026-11-20"
        }));
        let error = charge_for(&asset, "2026-10").expect_err("refused");
        assert!(
            error.to_string().contains("before the asset was acquired"),
            "{error}"
        );
    }

    #[test]
    fn time_based_refuses_a_month_after_the_life_has_run_out() {
        let asset = stored_asset(json!({
            "depreciation_method": "time_based",
            "acquisition_date": "2020-01-01",
            "useful_life_years": 5
        }));
        let error = charge_for(&asset, "2026-11").expect_err("refused");
        assert!(error.to_string().contains("life ended"), "{error}");
    }

    #[test]
    fn the_last_charge_is_what_is_left_rather_than_a_whole_month_of_it() {
        // 200000 of depreciable cost, 199999.80 already written off: the last charge is the twenty cents, not 3333.33
        let asset = stored_asset(json!({
            "accumulated_depreciation": "199999.80",
            "net_book_value": "40000.20"
        }));
        assert_eq!(charge_for(&asset, "2026-11").unwrap().to_fixed(2), "0.20");
    }

    #[test]
    fn depreciating_moves_the_asset_and_records_the_entry() {
        let graph = FakeGraph::returning(vec![
            stored_asset(json!({})),
            stored_asset(
                json!({"accumulated_depreciation": "3333.33", "net_book_value": "236666.67"}),
            ),
            json!({"id": "d-1", "asset_id": "a-1", "period": "2026-09",
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
    fn a_month_is_charged_once_and_charging_it_again_is_refused() {
        // the register holds this month already: charging it a second time would write the asset down twice, and two rows
        // for one month read as two months
        let graph = FakeGraph::returning(vec![
            stored_asset(json!({})),
            json!({"id": "d-1", "asset_id": "a-1", "period": "2026-10",
                   "depreciation_amount": "3333.33", "accumulated_depreciation": "3333.33",
                   "net_book_value": "236666.67", "method": "straight_line"}),
        ]);
        let error = depreciate(&graph, &scope(), "a-1", "2026-10").expect_err("refused");
        let said = format!("{error}");
        assert!(
            said.contains("already recorded for period 2026-10"),
            "{said}"
        );
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
        assert_eq!(summary["disposed_assets"], json!(1));
        // 240000 held + 10000 held; the sold row's 240000 cost is history, not something the business holds
        assert_eq!(summary["total_acquisition_cost"], json!("250000.00"));
        assert_eq!(summary["total_net_book_value"], json!("246000.00"));
        // the sold one is a row on the register and nothing more: one vehicle is held, not two
        assert_eq!(summary["by_category"]["vehicles"]["count"], json!(1));
        assert_eq!(
            summary["by_category"]["vehicles"]["nbv"],
            json!("240000.00")
        );
        assert_eq!(summary["by_category"]["IT"]["nbv"], json!("6000.00"));
    }

    #[test]
    fn a_sold_asset_is_not_part_of_what_the_register_is_worth() {
        let graph = FakeGraph::returning(vec![
            stored_asset(json!({"category": "vehicles", "net_book_value": "1000"})),
            stored_asset(json!({
                "id": "a-2",
                "category": "vehicles",
                "status": "disposed",
                "net_book_value": "3000"
            })),
        ]);
        let summary = summary(&graph, &scope()).expect("summary");
        assert_eq!(summary["total_net_book_value"], json!("1000.00"));
        assert_eq!(summary["total_acquisition_cost"], json!("240000.00"));
        assert_eq!(summary["active_assets"], json!(1));
        assert_eq!(summary["disposed_assets"], json!(1));
    }

    #[test]
    fn amounts_past_double_precision_keep_every_digit() {
        let graph = FakeGraph::returning(vec![stored_asset(json!({
            "acquisition_cost": "123456789012345678901.55",
            "net_book_value": "123456789012345678901.55",
            "salvage_value": "0",
            "useful_life_years": 100
        }))]);
        let charge = charge_for(
            &stored_asset(json!({
            "acquisition_cost": "123456789012345678901.55",
            "net_book_value": "123456789012345678901.55",
            "salvage_value": "0",
                "useful_life_years": 100
            })),
            "2026-11",
        )
        .unwrap();
        // 1200 months of service, so the charge is the cost over 1200
        assert_eq!(charge.to_fixed(2), "102880657510288065.75");
        assert_eq!(graph.statements(), 0);
    }
}
