#![forbid(unsafe_code)]
//! intangible-assets-service's records, on the store.
//!
//! An intangible behaves like a fixed asset with different vocabulary: it is amortised rather than
//! depreciated, and an asset written down to its residual value is `fully_amortized` rather than
//! `disposed`. The figures are `Money`, so a long cost does not drift while it is being written off.
//!
//! Kept as shipped, each pinned by a test:
//!
//! * The six asset types, and no others.
//! * Straight line charges `(cost - residual) / life` a year in twelfths; a zero life used to raise
//!   a division error and a 500, and now charges nothing, which is the only sensible monthly figure.
//! * Reducing balance charges 10% a year on what is left above the residual value, monthly, so it
//!   compounds and approaches the residual without reaching it.
//! * An asset at its residual value has its book value pinned there and its status set to
//!   `fully_amortized`.
//! * Only an `active` intangible can be amortised.

use serde_json::{json, Value};
use vimbai_common::Money;

use vimbai_store::amounts::{amount, stored_amount, wire};
use vimbai_store::read::required_text;
use vimbai_store::read::{text, whole_signed};
use vimbai_store::{BookScope, Graph, Records, StoreError, AMORTIZATION_ENTRY, INTANGIBLE_ASSET};

/// The six types the service accepted.
pub const TYPES: [&str; 6] = [
    "goodwill",
    "patent",
    "trademark",
    "copyright",
    "software",
    "license",
];

/// `/assets`: register an intangible, at cost.
pub fn create_asset<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let asset_type = required_text(payload, "asset_type")?;
    if !TYPES.contains(&asset_type.as_str()) {
        return Err(StoreError::Invalid(format!(
            "Invalid type. Must be one of {TYPES:?}"
        )));
    }
    let cost = amount(payload, "cost")?;
    let residual = match payload.get("residual_value") {
        Some(Value::Null) | None => Money::zero(),
        Some(_) => amount(payload, "residual_value")?,
    };
    let method = {
        let value = text(payload, "amortization_method");
        if value.is_empty() {
            "straight_line".to_string()
        } else {
            value
        }
    };

    Records::new(graph, INTANGIBLE_ASSET).create(
        scope,
        &json!({
            "name": required_text(payload, "name")?,
            "asset_type": asset_type,
            "cost": wire(&cost),
            "useful_life_years": whole_signed(payload, "useful_life_years")?,
            "residual_value": wire(&residual),
            "acquisition_date": required_text(payload, "acquisition_date")?,
            "amortization_method": method,
            "accumulated_amortization": wire(&Money::zero()),
            "net_book_value": wire(&cost),
            "status": "active",
        }),
    )
}

/// `/assets`: the caller's intangibles, optionally narrowed.
pub fn list_assets<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    asset_type: Option<&str>,
    status: Option<&str>,
) -> Result<Value, StoreError> {
    let rows = Records::new(graph, INTANGIBLE_ASSET).list(scope, 1000)?;
    let keep = |row: &Value, key: &str, wanted: Option<&str>| match wanted {
        None => true,
        Some(value) => row.get(key).and_then(Value::as_str) == Some(value),
    };
    Ok(Value::Array(
        rows.into_iter()
            .filter(|row| keep(row, "asset_type", asset_type))
            .filter(|row| keep(row, "status", status))
            .collect(),
    ))
}

pub fn get_asset<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    asset_id: &str,
) -> Result<Value, StoreError> {
    Records::new(graph, INTANGIBLE_ASSET).get(scope, asset_id)
}

/// The monthly amortisation for one intangible.
fn monthly_charge(asset: &Value) -> Result<Money, StoreError> {
    let method = asset
        .get("amortization_method")
        .and_then(Value::as_str)
        .unwrap_or("straight_line");
    let cost = stored_amount(asset, "cost")?;
    let residual = stored_amount(asset, "residual_value")?;
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
                cost.minus(&residual)
                    .over(&Money::from_i64(life * 12))
                    .round(2)
            }
        }
        _ => {
            // 10% a year on what is left above the residual value, charged monthly
            let remaining = net_book_value.minus(&residual);
            if remaining.is_zero() {
                Money::zero()
            } else {
                remaining
                    .times(&Money::from_i64(10))
                    .over(&Money::from_i64(1200))
                    .round(2)
            }
        }
    })
}

/// `/assets/{id}/amortize`: charge one period and record the entry.
pub fn amortize<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    asset_id: &str,
    period: &str,
) -> Result<Value, StoreError> {
    if period.trim().is_empty() {
        return Err(StoreError::Invalid("period is required".to_string()));
    }
    let assets = Records::new(graph, INTANGIBLE_ASSET);
    let asset = assets.get(scope, asset_id)?;

    let status = asset
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("active");
    if status != "active" {
        return Err(StoreError::Invalid(format!(
            "Asset is {status}, cannot amortize"
        )));
    }

    let cost = stored_amount(&asset, "cost")?;
    let residual = stored_amount(&asset, "residual_value")?;
    let charge = monthly_charge(&asset)?;

    let accumulated = stored_amount(&asset, "accumulated_amortization")?.plus(&charge);
    let mut net_book_value = cost.minus(&accumulated);
    let mut new_status = "active";
    if net_book_value.to_f64_lossy() <= residual.to_f64_lossy() {
        net_book_value = residual.clone();
        new_status = "fully_amortized";
    }

    assets.update(
        scope,
        asset_id,
        &json!({
            "accumulated_amortization": wire(&accumulated),
            "net_book_value": wire(&net_book_value),
            "status": new_status,
        }),
    )?;

    Records::new(graph, AMORTIZATION_ENTRY).create(
        scope,
        &json!({
            "asset_id": asset_id,
            "period": period,
            "amortization_amount": wire(&charge),
            "accumulated_amortization": wire(&accumulated),
            "net_book_value": wire(&net_book_value),
        }),
    )
}

/// `/assets/{id}/amortization`: the entries charged against one intangible.
pub fn list_entries<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    asset_id: &str,
) -> Result<Value, StoreError> {
    let rows = Records::new(graph, AMORTIZATION_ENTRY).list(scope, 1000)?;
    Ok(Value::Array(
        rows.into_iter()
            .filter(|row| row.get("asset_id").and_then(Value::as_str) == Some(asset_id))
            .collect(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::FakeGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn stored(fields: Value) -> Value {
        let base = json!({
            "id": "i-1",
            "name": "Brand",
            "asset_type": "trademark",
            "cost": "48000",
            "useful_life_years": 4,
            "residual_value": "0",
            "acquisition_date": "2024-01-01",
            "amortization_method": "straight_line",
            "accumulated_amortization": "0",
            "net_book_value": "48000",
            "status": "active"
        });
        let mut merged = base.as_object().unwrap().clone();
        for (key, value) in fields.as_object().unwrap() {
            merged.insert(key.clone(), value.clone());
        }
        Value::Object(merged)
    }

    #[test]
    fn the_six_types_are_enforced() {
        let graph = FakeGraph::returning(vec![stored(json!({}))]);
        let refused = create_asset(
            &graph,
            &scope(),
            &json!({"name": "X", "asset_type": "brand", "cost": "1",
                    "useful_life_years": 1, "acquisition_date": "2024-01-01"}),
        )
        .expect_err("refused");
        assert!(refused.to_string().contains("Invalid type"), "{refused}");

        let ok = create_asset(
            &graph,
            &scope(),
            &json!({"name": "Goodwill", "asset_type": "goodwill", "cost": "1000",
                    "useful_life_years": 5, "acquisition_date": "2024-01-01"}),
        );
        assert!(ok.is_ok(), "{:?}", ok.err());
    }

    #[test]
    fn straight_line_charges_cost_over_the_months() {
        // 48000 over 4 years = 1000 a month
        assert_eq!(
            monthly_charge(&stored(json!({}))).unwrap().to_fixed(2),
            "1000.00"
        );
    }

    #[test]
    fn a_zero_life_charges_nothing_instead_of_failing() {
        assert_eq!(
            monthly_charge(&stored(json!({"useful_life_years": 0}))).unwrap(),
            Money::zero()
        );
    }

    #[test]
    fn reducing_balance_charges_ten_percent_a_year_on_what_is_left() {
        let asset = stored(json!({
            "amortization_method": "reducing_balance",
            "net_book_value": "12000",
            "residual_value": "2000"
        }));
        // 10% of (12000 - 2000) over twelve months
        assert_eq!(monthly_charge(&asset).unwrap().to_fixed(2), "83.33");
    }

    #[test]
    fn amortising_moves_the_asset_and_records_the_entry() {
        let graph = FakeGraph::returning(vec![stored(json!({}))]);
        amortize(&graph, &scope(), "i-1", "2026-10").expect("entry");
        let (cypher, params) = graph.last();
        assert!(cypher.contains("AmortizationEntry"), "{cypher}");
        assert_eq!(params["props"]["amortization_amount"], json!("1000.000000"));
        assert_eq!(
            params["props"]["accumulated_amortization"],
            json!("1000.000000")
        );
        assert_eq!(params["props"]["net_book_value"], json!("47000.000000"));
    }

    #[test]
    fn an_intangible_at_its_residual_is_fully_amortized() {
        let graph = FakeGraph::returning(vec![stored(json!({
            "residual_value": "500",
            "accumulated_amortization": "47000",
            "net_book_value": "1000"
        }))]);
        amortize(&graph, &scope(), "i-1", "2026-10").expect("entry");
        let (_, params) = graph.last();
        // 1000 - 1000 reaches the 500 residual, so the value is pinned there
        assert_eq!(params["props"]["net_book_value"], json!("500.000000"));
    }

    #[test]
    fn a_fully_amortized_intangible_is_not_charged_again() {
        let graph = FakeGraph::returning(vec![stored(json!({"status": "fully_amortized"}))]);
        let error = amortize(&graph, &scope(), "i-1", "2026-11").expect_err("refused");
        assert_eq!(
            error.to_string(),
            "Asset is fully_amortized, cannot amortize"
        );
    }
}
