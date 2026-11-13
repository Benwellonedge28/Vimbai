#![forbid(unsafe_code)]
//! balance-sheet-service, on the store.
//!
//! What a company owns, owes and is worth on a date, with the totals computed here rather than sent in.
//! Every sheet is kept, so the latest one and the history both come off the record.
//!
//! The arithmetic, all exact:
//!
//! * each total is the sum of its own items,
//! * the sheet balances when assets equal liabilities plus equity, to within a cent.
//!
//! Kept as shipped: the ratio endpoint floors every denominator at 1, so a company with no
//! liabilities reports a debt-to-equity ratio equal to its liabilities rather than dividing by zero
//! and so does one with no assets. A missing sheet is a 404, not an empty sheet.
//!
//! This is one implementation, not two: the float copy that used to sit in the ratios crate was
//! balanced the same way in doubles and nothing reached it from any door, so it was deleted when this
//! landed rather than left to drift from these figures.

use serde_json::{json, Value};
use vimbai_common::Money;

use vimbai_store::amounts::{amount_or_zero, stored_amount, wire};
use vimbai_store::read::text;
use vimbai_store::{BookScope, Graph, Records, StoreError, BALANCE_SHEET};

/// A cent, the tolerance the sheet balances to.
fn cent() -> Money {
    Money::parse("0.01").expect("a cent")
}

fn items(payload: &Value, key: &str) -> Vec<Value> {
    payload
        .get(key)
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn sum_of(rows: &[Value]) -> Result<Money, StoreError> {
    let mut total = Money::zero();
    for row in rows {
        total = total.plus(&amount_or_zero(row, "amount")?);
    }
    Ok(total)
}

/// The three totals and whether the sheet balances.
pub fn totals(
    assets: &[Value],
    liabilities: &[Value],
    equity: &[Value],
) -> Result<Totals, StoreError> {
    let total_assets = sum_of(assets)?;
    let total_liabilities = sum_of(liabilities)?;
    let total_equity = sum_of(equity)?;
    let right = total_liabilities.plus(&total_equity);
    let gap = if total_assets > right {
        total_assets.minus(&right)
    } else {
        right.minus(&total_assets)
    };
    Ok(Totals {
        assets: total_assets,
        liabilities: total_liabilities,
        equity: total_equity,
        balanced: gap < cent(),
    })
}

pub struct Totals {
    pub assets: Money,
    pub liabilities: Money,
    pub equity: Money,
    pub balanced: bool,
}

/// A ratio, floored at the divisor of 1 so a missing denominator reports rather than divides.
fn ratio(numerator: &Money, denominator: &Money) -> Money {
    let one = Money::parse("1").expect("one");
    let divisor = if *denominator < one {
        one
    } else {
        denominator.clone()
    };
    numerator.over(&divisor)
}

/// `/generate`.
pub fn generate<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let assets = items(payload, "assets");
    let liabilities = items(payload, "liabilities");
    let equity = items(payload, "equity");
    let computed = totals(&assets, &liabilities, &equity)?;
    Records::new(graph, BALANCE_SHEET).create(
        scope,
        &json!({
            "company_id": text(payload, "company_id"),
            "as_of_date": text(payload, "as_of_date"),
            "assets": assets,
            "liabilities": liabilities,
            "equity": equity,
            "total_assets": wire(&computed.assets),
            "total_liabilities": wire(&computed.liabilities),
            "total_equity": wire(&computed.equity),
            "is_balanced": computed.balanced,
        }),
    )
}

fn history_of<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    company_id: &str,
) -> Result<Vec<Value>, StoreError> {
    let mut rows: Vec<Value> = Records::new(graph, BALANCE_SHEET)
        .list(scope, 1000)?
        .into_iter()
        .filter(|row| row.get("company_id").and_then(Value::as_str) == Some(company_id))
        .collect();
    // oldest first, so the last one is the latest sheet
    rows.sort_by_key(|row| {
        row.get("created_at")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    });
    Ok(rows)
}

/// `/history/{company_id}`.
pub fn history<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    company_id: &str,
) -> Result<Value, StoreError> {
    let rows = history_of(graph, scope, company_id)?;
    Ok(json!({
        "company_id": company_id,
        "sheets": rows,
        "total": rows.len(),
    }))
}

/// `/latest/{company_id}` and `/ratios/{company_id}` both need the most recent sheet.
fn latest<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    company_id: &str,
) -> Result<Value, StoreError> {
    history_of(graph, scope, company_id)?
        .pop()
        .ok_or(StoreError::NotFound)
}

/// `/latest/{company_id}`.
pub fn latest_sheet<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    company_id: &str,
) -> Result<Value, StoreError> {
    latest(graph, scope, company_id)
}

/// `/ratios/{company_id}`: read off the latest sheet, every denominator floored at 1.
pub fn ratios<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    company_id: &str,
) -> Result<Value, StoreError> {
    let sheet = latest(graph, scope, company_id)?;
    let assets = sheet
        .get("assets")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let liabilities = sheet
        .get("liabilities")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let mut current_assets = Money::zero();
    let mut liquid_assets = Money::zero();
    for row in &assets {
        let amount = amount_or_zero(row, "amount")?;
        if row.get("category").and_then(Value::as_str) == Some("current") {
            current_assets = current_assets.plus(&amount);
        }
        if row
            .get("is_liquid")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            liquid_assets = liquid_assets.plus(&amount);
        }
    }
    let mut current_liabilities = Money::zero();
    for row in &liabilities {
        if row.get("category").and_then(Value::as_str) == Some("current") {
            current_liabilities = current_liabilities.plus(&amount_or_zero(row, "amount")?);
        }
    }

    let total_liabilities = stored_amount(&sheet, "total_liabilities")?;
    let total_equity = stored_amount(&sheet, "total_equity")?;
    let total_assets = stored_amount(&sheet, "total_assets")?;

    Ok(json!({
        "current_ratio": wire(&ratio(&current_assets, &current_liabilities)),
        "quick_ratio": wire(&ratio(&liquid_assets, &current_liabilities)),
        "debt_to_equity": wire(&ratio(&total_liabilities, &total_equity)),
        "debt_to_assets": wire(&ratio(&total_liabilities, &total_assets)),
        "equity_ratio": wire(&ratio(&total_equity, &total_assets)),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::FakeGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn sheet(total_assets: &str, total_liabilities: &str, total_equity: &str) -> Value {
        let stamp = format!("2026-03-0{}T00:00:00", 1 + total_assets.len() % 8);
        json!({
            "id": "sheet-1", "company_id": "co-1", "created_at": stamp,
            "assets": [{"name": "Cash", "amount": "600", "category": "current", "is_liquid": true}],
            "liabilities": [{"name": "Loan", "amount": "400", "category": "current"}],
            "total_assets": total_assets,
            "total_liabilities": total_liabilities,
            "total_equity": total_equity,
        })
    }

    #[test]
    fn the_totals_are_the_sums_of_their_own_items() {
        let assets = vec![json!({"amount": "600"}), json!({"amount": "400"})];
        let liabilities = vec![json!({"amount": "250"})];
        let equity = vec![json!({"amount": "750"})];
        let computed = totals(&assets, &liabilities, &equity).expect("totals");
        assert_eq!(wire(&computed.assets), "1000.000000");
        assert_eq!(wire(&computed.liabilities), "250.000000");
        assert_eq!(wire(&computed.equity), "750.000000");
        assert!(computed.balanced);
    }

    #[test]
    fn a_sheet_balances_to_within_a_cent() {
        let assets = vec![json!({"amount": "100.005"})];
        let none: Vec<Value> = Vec::new();
        let equity = vec![json!({"amount": "100"})];
        assert!(totals(&assets, &none, &equity).expect("totals").balanced);
        let far = vec![json!({"amount": "100.5"})];
        assert!(!totals(&far, &none, &equity).expect("totals").balanced);
    }

    #[test]
    fn totals_past_double_size_keep_every_digit() {
        let assets = vec![
            json!({"amount": "123456789012345678901.55"}),
            json!({"amount": "0.45"}),
        ];
        let liabilities = vec![json!({"amount": "123456789012345678902"})];
        let computed = totals(&assets, &liabilities, &[]).expect("totals");
        assert_eq!(wire(&computed.assets), "123456789012345678902.000000");
        assert!(computed.balanced);
    }

    #[test]
    fn generating_a_sheet_records_the_totals_the_caller_did_not_send() {
        let graph = FakeGraph::returning(vec![json!({"id": "sheet-1"})]);
        generate(
            &graph,
            &scope(),
            &json!({"company_id": "co-1", "as_of_date": "2026-03-01",
                    "assets": [{"name": "Cash", "amount": "600", "category": "current", "is_liquid": true}],
                    "liabilities": [{"name": "Loan", "amount": "400", "category": "current"}],
                    "equity": [{"name": "Capital", "amount": "200"}]}),
        )
        .expect("generated");
        let (_, params) = graph.last();
        assert_eq!(params["props"]["total_assets"], json!("600.000000"));
        assert_eq!(params["props"]["is_balanced"], json!(true));
    }

    #[test]
    fn no_sheet_at_all_is_not_found_rather_than_an_empty_sheet() {
        let graph = FakeGraph::returning(Vec::new());
        assert!(matches!(
            latest_sheet(&graph, &scope(), "co-1"),
            Err(StoreError::NotFound)
        ));
        assert!(matches!(
            ratios(&graph, &scope(), "co-1"),
            Err(StoreError::NotFound)
        ));
    }

    #[test]
    fn the_ratios_read_the_latest_sheet_and_floor_each_denominator() {
        let graph = FakeGraph::returning(vec![
            sheet("1000", "400", "600"),
            sheet("2000", "0", "2000"),
        ]);
        let answer = ratios(&graph, &scope(), "co-1").expect("ratios");
        // the newer sheet has no liabilities, so debt to equity reports zero against a divisor of 1
        assert_eq!(answer["debt_to_equity"], json!("0.000000"));
        assert_eq!(answer["debt_to_assets"], json!("0.000000"));
        assert_eq!(answer["equity_ratio"], json!("1.000000"));
        // the current items on that sheet are 600 of cash against the 400 loan
        assert_eq!(answer["current_ratio"], json!("1.500000"));
        assert_eq!(answer["quick_ratio"], json!("1.500000"));
    }

    #[test]
    fn the_latest_sheet_is_the_one_recorded_last() {
        let graph = FakeGraph::returning(vec![
            sheet("1000", "400", "600"),
            sheet("2000", "0", "2000"),
        ]);
        let answer = latest_sheet(&graph, &scope(), "co-1").expect("latest");
        assert_eq!(answer["total_assets"], json!("2000"));
    }
}
