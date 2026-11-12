//! trading-account-service's generated accounts, on the store.
//!
//! A trading account is a record, not a transient sum: every generation is stored as its own node
//! and can be read back later, which is why this lives here rather than in a calculator crate.
//!
//! The arithmetic is the service's, unchanged: net sales less returns, cost of goods sold across
//! opening stock, purchases, carriage inward, direct wages and factory overhead, less closing
//! stock; gross profit is the difference, and its percentage is the profit over net sales rounded
//! to two places, or zero when there are no net sales. It runs on `Money`, so a large stock
//! valuation does not drift while it is being added up.

#![forbid(unsafe_code)]

use serde_json::{json, Value};
use vimbai_common::Money;

use vimbai_store::amounts::{stored_amount, wire};
use vimbai_store::{BookScope, Graph, Records, StoreError, TRADING_ACCOUNT};

fn field(payload: &Value, key: &str) -> Result<Money, StoreError> {
    match payload.get(key) {
        Some(Value::Number(number)) => Money::from_f64_shortest(number.as_f64().unwrap_or(0.0))
            .map_err(|error| StoreError::Invalid(format!("{key}: {error}"))),
        Some(Value::String(text)) => {
            Money::parse(text).map_err(|error| StoreError::Invalid(format!("{key}: {error}")))
        }
        None | Some(Value::Null) => Ok(Money::zero()),
        Some(_) => Err(StoreError::Invalid(format!("{key} is not an amount"))),
    }
}

/// What the account says, given its inputs.
struct Figures {
    net_sales: Money,
    cost_of_goods_sold: Money,
    gross_profit: Money,
    gross_profit_pct: Money,
    details: Value,
}

fn compute(payload: &Value) -> Result<Figures, StoreError> {
    let opening_stock = field(payload, "opening_stock")?;
    let purchases = field(payload, "purchases")?;
    let carriage_inward = field(payload, "carriage_inward")?;
    let closing_stock = field(payload, "closing_stock")?;
    let sales = field(payload, "sales")?;
    let sales_returns = field(payload, "sales_returns")?;
    let direct_wages = field(payload, "direct_wages")?;
    let factory_overhead = field(payload, "factory_overhead")?;

    let net_sales = sales.minus(&sales_returns);
    let cost_of_goods_sold = opening_stock
        .plus(&purchases)
        .plus(&carriage_inward)
        .plus(&direct_wages)
        .plus(&factory_overhead)
        .minus(&closing_stock);
    let gross_profit = net_sales.minus(&cost_of_goods_sold);
    // No net sales means no meaningful percentage, which the service reported as zero.
    let gross_profit_pct = if net_sales.is_zero() {
        Money::zero()
    } else {
        gross_profit
            .over(&net_sales)
            .times(&Money::from_i64(100))
            .round(2)
    };

    Ok(Figures {
        net_sales,
        cost_of_goods_sold,
        gross_profit,
        gross_profit_pct,
        details: json!({
            "opening_stock": wire(&opening_stock),
            "purchases": wire(&purchases),
            "carriage_inward": wire(&carriage_inward),
            "closing_stock": wire(&closing_stock),
            "sales": wire(&sales),
            "sales_returns": wire(&sales_returns),
            "direct_wages": wire(&direct_wages),
            "factory_overhead": wire(&factory_overhead),
        }),
    })
}

/// `/generate`: compute the account and store it.
pub fn generate<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let figures = compute(payload)?;
    let company_id = payload
        .get("company_id")
        .and_then(Value::as_str)
        .unwrap_or("");
    let period = payload.get("period").and_then(Value::as_str).unwrap_or("");

    let stored = Records::new(graph, TRADING_ACCOUNT).create(
        scope,
        &json!({
            "company_id": company_id,
            "period": period,
            "net_sales": figures.net_sales.to_fixed(6),
            "cost_of_goods_sold": figures.cost_of_goods_sold.to_fixed(6),
            "gross_profit": figures.gross_profit.to_fixed(6),
            "gross_profit_pct": figures.gross_profit_pct.to_fixed(6),
            "details_json": serde_json::to_string(&figures.details).unwrap_or_else(|_| "{}".into()),
            "created_at": payload.get("created_at").and_then(Value::as_str).unwrap_or(""),
        }),
    )?;

    result_of(&stored)
}

fn result_of(row: &Value) -> Result<Value, StoreError> {
    let details: Value = row
        .get("details_json")
        .and_then(Value::as_str)
        .and_then(|text| serde_json::from_str(text).ok())
        .unwrap_or_else(|| json!({}));
    Ok(json!({
        "id": row.get("id").cloned().unwrap_or(Value::Null),
        "company_id": row.get("company_id").cloned().unwrap_or(Value::Null),
        "period": row.get("period").cloned().unwrap_or(Value::Null),
        "net_sales": wire(&stored_amount(row, "net_sales")?),
        "cost_of_goods_sold": wire(&stored_amount(row, "cost_of_goods_sold")?),
        "gross_profit": wire(&stored_amount(row, "gross_profit")?),
        "gross_profit_pct": wire(&stored_amount(row, "gross_profit_pct")?),
        "details": details,
        "created_at": row.get("created_at").cloned().unwrap_or(Value::Null),
    }))
}

/// `/accounts`: every account the caller can see, oldest first, optionally one period.
pub fn list<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    period: Option<&str>,
) -> Result<Value, StoreError> {
    let mut rows = Records::new(graph, TRADING_ACCOUNT).list(scope, 500)?;
    rows.sort_by_key(|row| {
        row.get("created_at")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned()
    });
    let mut out = Vec::new();
    for row in &rows {
        if let Some(wanted) = period {
            if row.get("period").and_then(Value::as_str) != Some(wanted) {
                continue;
            }
        }
        out.push(result_of(row)?);
    }
    Ok(Value::Array(out))
}

/// `/accounts/{account_id}`: one account, or NotFound.
pub fn get<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    account_id: &str,
) -> Result<Value, StoreError> {
    let row = Records::new(graph, TRADING_ACCOUNT).get(scope, account_id)?;
    result_of(&row)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_stock_and_the_profit_tie_out() {
        let figures = compute(&json!({
            "opening_stock": 10000,
            "purchases": 40000,
            "carriage_inward": 500,
            "closing_stock": 8000,
            "sales": 60000,
            "sales_returns": 1000,
            "direct_wages": 5000,
            "factory_overhead": 2500,
        }))
        .unwrap();
        assert_eq!(figures.net_sales.to_fixed(2), "59000.00");
        // 10000 + 40000 + 500 + 5000 + 2500 - 8000
        assert_eq!(figures.cost_of_goods_sold.to_fixed(2), "50000.00");
        assert_eq!(figures.gross_profit.to_fixed(2), "9000.00");
        // 9000 / 59000 * 100, rounded to two places
        assert_eq!(figures.gross_profit_pct.to_fixed(2), "15.25");
    }

    #[test]
    fn no_net_sales_reports_zero_rather_than_dividing_by_it() {
        let figures = compute(&json!({"sales": 0, "sales_returns": 0})).unwrap();
        assert_eq!(figures.gross_profit_pct.to_fixed(2), "0.00");
    }

    #[test]
    fn a_large_stock_valuation_stays_exact() {
        let figures = compute(&json!({
            "opening_stock": "12345678901234.56",
            "purchases": "0.01",
            "sales": "20000000000000",
        }))
        .unwrap();
        assert_eq!(figures.cost_of_goods_sold.to_fixed(2), "12345678901234.57");
    }

    #[test]
    fn an_unknown_account_is_not_found() {
        let graph = vimbai_store::fake::FakeGraph::returning(vec![]);
        assert_eq!(
            get(&graph, &BookScope::new("user-1", "book-1"), "missing").unwrap_err(),
            StoreError::NotFound
        );
    }
}
