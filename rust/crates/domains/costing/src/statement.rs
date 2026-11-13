#![forbid(unsafe_code)]
//! absorption-costing-statement-service: the trading account and the production cost statement.
//!
//! Two statements, each built from the figures a caller gives and then kept: a trading account that runs from sales
//! down to gross profit and, once the expenses are added, to net profit; and a production cost statement that runs
//! from materials through prime cost to the production cost and the cost of each unit.
//!
//! Behaviours kept as shipped, each pinned by a test:
//!
//! * the cost of goods sold is opening stock plus purchases plus carriage inwards less closing stock, and the gross
//!   profit is sales less that - a closing stock larger than everything bought simply carries the gross profit up;
//! * the line items are built in the shipped order, with sales first and the totals flagged as totals, so a statement
//!   reads the way the service printed it;
//! * adding expenses appends the profit section to the statement it finds, marks it completed and rewrites it, so a
//!   second call appends a second section and the later figures win;
//! * adding expenses to a statement the caller does not hold is a miss rather than a new statement;
//! * the production cost is prime cost plus overhead plus opening work in progress less closing work in progress;
//! * the cost per unit is left at nil rather than dividing when nothing was produced;
//! * both listings filter by the field they were asked about and report their rows under `statements`.

use serde_json::{json, Value};

use vimbai_common::Money;
use vimbai_store::amounts::wire;
use vimbai_store::read::{decimal, field, optional, refused, text};
use vimbai_store::write::remember;
use vimbai_store::{
    BookScope, Graph, Records, StoreError, PRODUCTION_COST_STATEMENT, TRADING_ACCOUNT_STATEMENT,
};

fn amount(payload: &Value, key: &str) -> Result<Money, StoreError> {
    Money::parse(&decimal(payload, key, "0"))
        .map_err(|_| refused(&format!("{key} must be an amount")))
}

fn line(description: &str, amount: &Money, flag: &str, indent: i64) -> Value {
    let mut row =
        json!({"description": description, "amount": wire(amount), "indent_level": indent});
    if let Some(fields) = row.as_object_mut() {
        fields.insert(flag.to_string(), json!(true));
    }
    row
}

fn find(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    kind: vimbai_store::RecordKind,
    id: &str,
) -> Result<Value, StoreError> {
    Records::new(graph, kind)
        .list(scope, 2000)?
        .into_iter()
        .find(|row| field(row, "id") == id)
        .ok_or(StoreError::NotFound)
}

/// `POST /trading-account/generate`.
pub fn generate_trading_statement(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let company = text(payload, "company_id");
    if company.is_empty() {
        return Err(refused("company_id is required"));
    }
    let period_start = text(payload, "period_start");
    let period_end = text(payload, "period_end");
    if period_start.is_empty() || period_end.is_empty() {
        return Err(refused("period_start and period_end are required"));
    }

    let opening_stock = amount(payload, "opening_stock")?;
    let purchases = amount(payload, "purchases")?;
    let carriage_inwards = amount(payload, "carriage_inwards")?;
    let closing_stock = amount(payload, "closing_stock")?;
    let sales_revenue = amount(payload, "sales_revenue")?;

    let goods_available = opening_stock.plus(&purchases).plus(&carriage_inwards);
    let cost_of_goods_sold = goods_available.minus(&closing_stock);
    let gross_profit = sales_revenue.minus(&cost_of_goods_sold);

    let line_items = json!([
        line("Sales Revenue", &sales_revenue, "is_total", 0),
        line("Opening Stock", &opening_stock, "indent_level", 1),
        line("Add: Purchases", &purchases, "indent_level", 1),
        line(
            "Add: Carriage Inwards",
            &carriage_inwards,
            "indent_level",
            1
        ),
        line(
            "Cost of Goods Available",
            &goods_available,
            "is_subtotal",
            1
        ),
        line("Less: Closing Stock", &closing_stock, "indent_level", 1),
        line("Cost of Goods Sold", &cost_of_goods_sold, "is_total", 0),
        line("Gross Profit", &gross_profit, "is_total", 0),
    ]);

    remember(
        graph,
        scope,
        TRADING_ACCOUNT_STATEMENT,
        json!({
            "company_id": company,
            "period_start": period_start,
            "period_end": period_end,
            "opening_stock": wire(&opening_stock),
            "purchases": wire(&purchases),
            "carriage_inwards": wire(&carriage_inwards),
            "closing_stock": wire(&closing_stock),
            "sales_revenue": wire(&sales_revenue),
            "cost_of_goods_sold": wire(&cost_of_goods_sold),
            "gross_profit": wire(&gross_profit),
            "distribution_costs": wire(&Money::zero()),
            "administrative_expenses": wire(&Money::zero()),
            "other_expenses": wire(&Money::zero()),
            "query_net_profit": Value::Null,
            "line_items": line_items,
            "status": "draft",
        }),
    )
}

/// `POST /trading-account/{statement_id}/add-expenses`.
pub fn add_statement_expenses(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    statement_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let statement = find(graph, scope, TRADING_ACCOUNT_STATEMENT, statement_id)?;

    let distribution = amount(payload, "distribution_costs")?;
    let administrative = amount(payload, "administrative_expenses")?;
    let other = amount(payload, "other_expenses")?;
    let gross_profit =
        Money::parse(&decimal(&statement, "gross_profit", "0")).unwrap_or_else(|_| Money::zero());
    let net_profit = gross_profit
        .minus(&distribution)
        .minus(&administrative)
        .minus(&other);

    let mut line_items = statement
        .get("line_items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    line_items.extend([
        line("Gross Profit", &gross_profit, "is_total", 0),
        line("Less: Distribution Costs", &distribution, "indent_level", 1),
        line(
            "Less: Administrative Expenses",
            &administrative,
            "indent_level",
            1,
        ),
        line("Less: Other Expenses", &other, "indent_level", 1),
        line("Net Profit", &net_profit, "is_total", 0),
    ]);

    // the statement is rewritten in place, so the expenses land on the one the caller asked about
    let mut updates = json!({
        "distribution_costs": wire(&distribution),
        "administrative_expenses": wire(&administrative),
        "other_expenses": wire(&other),
        "net_profit": wire(&net_profit),
        "line_items": line_items,
        "status": "completed",
    });
    if let Some(fields) = updates.as_object_mut() {
        fields.remove("query_net_profit");
    }
    // the rewritten statement is returned whole, so the caller sees the figures it just added
    let stored =
        Records::new(graph, TRADING_ACCOUNT_STATEMENT).update(scope, statement_id, &updates)?;
    // the store answers with the row, or with a list holding it
    let mut answer = match stored {
        Value::Array(rows) => rows.into_iter().next().unwrap_or(Value::Null),
        other => other,
    };
    if let (Some(fields), Some(applied)) = (answer.as_object_mut(), updates.as_object()) {
        for (key, value) in applied {
            fields.insert(key.clone(), value.clone());
        }
    }
    Ok(answer)
}

/// `POST /production-cost/generate`.
pub fn generate_production_statement(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let product = text(payload, "product_id");
    if product.is_empty() {
        return Err(refused("product_id is required"));
    }
    let period = text(payload, "period");
    if period.is_empty() {
        return Err(refused("period is required"));
    }

    let materials_opening = amount(payload, "direct_materials_opening")?;
    let materials_purchases = amount(payload, "direct_materials_purchases")?;
    let materials_closing = amount(payload, "direct_materials_closing")?;
    let direct_labor = amount(payload, "direct_labor")?;
    let direct_expenses = amount(payload, "direct_expenses")?;
    let overhead = amount(payload, "factory_overhead")?;
    let wip_opening = amount(payload, "work_in_progress_opening")?;
    let wip_closing = amount(payload, "work_in_progress_closing")?;
    let units = payload
        .get("units_produced")
        .and_then(Value::as_i64)
        .unwrap_or(0);

    let materials_used = materials_opening
        .plus(&materials_purchases)
        .minus(&materials_closing);
    let prime_cost = materials_used.plus(&direct_labor).plus(&direct_expenses);
    let production_cost = prime_cost
        .plus(&overhead)
        .plus(&wip_opening)
        .minus(&wip_closing);
    // nothing produced means no cost per unit rather than a division
    let cost_per_unit = if units > 0 {
        production_cost.over(&Money::from_i64(units))
    } else {
        Money::zero()
    };

    remember(
        graph,
        scope,
        PRODUCTION_COST_STATEMENT,
        json!({
            "product_id": product,
            "period": period,
            "direct_materials_opening": wire(&materials_opening),
            "direct_materials_purchases": wire(&materials_purchases),
            "direct_materials_closing": wire(&materials_closing),
            "direct_materials_used": wire(&materials_used),
            "direct_labor": wire(&direct_labor),
            "direct_expenses": wire(&direct_expenses),
            "prime_cost": wire(&prime_cost),
            "factory_overhead": wire(&overhead),
            "work_in_progress_opening": wire(&wip_opening),
            "work_in_progress_closing": wire(&wip_closing),
            "production_cost": wire(&production_cost),
            "units_produced": units,
            "cost_per_unit": wire(&cost_per_unit),
        }),
    )
}

/// `GET /trading-account`.
pub fn list_trading_statements(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let company = optional(query, "company_id").unwrap_or_default();
    let statements: Vec<Value> = Records::new(graph, TRADING_ACCOUNT_STATEMENT)
        .list(scope, 2000)?
        .into_iter()
        .filter(|row| company.is_empty() || field(row, "company_id") == company)
        .collect();
    Ok(json!({"statements": statements}))
}

/// `GET /production-cost`.
pub fn list_production_statements(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let product = optional(query, "product_id").unwrap_or_default();
    let period = optional(query, "period").unwrap_or_default();
    let statements: Vec<Value> = Records::new(graph, PRODUCTION_COST_STATEMENT)
        .list(scope, 2000)?
        .into_iter()
        .filter(|row| product.is_empty() || field(row, "product_id") == product)
        .filter(|row| period.is_empty() || field(row, "period") == period)
        .collect();
    Ok(json!({"statements": statements}))
}

/// `GET /trading-account/{statement_id}`.
pub fn get_trading_statement(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    statement_id: &str,
) -> Result<Value, StoreError> {
    find(graph, scope, TRADING_ACCOUNT_STATEMENT, statement_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::FakeGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    #[test]
    fn a_trading_account_runs_from_sales_to_gross_profit() {
        let graph = FakeGraph::returning(vec![json!({"id": "t-1"})]);
        let answer = generate_trading_statement(
            &graph,
            &scope(),
            &json!({"company_id": "c-1", "period_start": "2026-01-01", "period_end": "2026-12-31",
                    "opening_stock": "1000", "purchases": "5000", "carriage_inwards": "200",
                    "closing_stock": "1200", "sales_revenue": "9000"}),
        )
        .expect("generated");
        // 1,000 + 5,000 + 200 - 1,200
        assert_eq!(answer["cost_of_goods_sold"], json!("5000.000000"));
        assert_eq!(answer["gross_profit"], json!("4000.000000"));
        assert_eq!(answer["status"], json!("draft"));
        assert_eq!(answer["line_items"].as_array().unwrap().len(), 8);
    }

    #[test]
    fn a_closing_stock_bigger_than_the_purchases_carries_the_gross_profit_up() {
        let graph = FakeGraph::returning(vec![json!({"id": "t-1"})]);
        let answer = generate_trading_statement(
            &graph,
            &scope(),
            &json!({"company_id": "c-1", "period_start": "2026-01-01", "period_end": "2026-12-31",
                    "opening_stock": "0", "purchases": "100", "carriage_inwards": "0",
                    "closing_stock": "400", "sales_revenue": "1000"}),
        )
        .expect("generated");
        assert_eq!(answer["cost_of_goods_sold"], json!("-300.000000"));
        assert_eq!(answer["gross_profit"], json!("1300.000000"));
    }

    #[test]
    fn adding_expenses_completes_the_statement_and_appends_the_profit_section() {
        let graph = FakeGraph::queue(vec![
            vec![json!({"id": "t-1", "gross_profit": "4000.000000",
                        "line_items": [{"description": "Sales Revenue", "amount": "9000.000000"}]})],
            // a single-key row is unwrapped to its value, so the answer carries the id beside the status
            vec![json!({"id": "t-1", "status": "draft"})],
        ]);
        let answer = add_statement_expenses(
            &graph,
            &scope(),
            "t-1",
            &json!({"distribution_costs": "500", "administrative_expenses": "700", "other_expenses": "100"}),
        )
        .expect("completed");
        assert_eq!(answer["net_profit"], json!("2700.000000"));
        assert_eq!(answer["line_items"].as_array().unwrap().len(), 6);
    }

    #[test]
    fn adding_expenses_to_a_statement_the_caller_does_not_hold_is_a_miss() {
        let graph = FakeGraph::returning(vec![]);
        assert!(matches!(
            add_statement_expenses(&graph, &scope(), "other", &json!({})),
            Err(StoreError::NotFound)
        ));
    }

    #[test]
    fn a_production_cost_statement_walks_from_materials_to_the_cost_of_each_unit() {
        let graph = FakeGraph::returning(vec![json!({"id": "p-1"})]);
        let answer = generate_production_statement(
            &graph,
            &scope(),
            &json!({"product_id": "pr-1", "period": "2026-01",
                    "direct_materials_opening": "1000", "direct_materials_purchases": "4000",
                    "direct_materials_closing": "800", "direct_labor": "2000", "direct_expenses": "300",
                    "factory_overhead": "1500", "work_in_progress_opening": "400",
                    "work_in_progress_closing": "600", "units_produced": 10}),
        )
        .expect("generated");
        // 1,000 + 4,000 - 800
        assert_eq!(answer["direct_materials_used"], json!("4200.000000"));
        // 4,200 + 2,000 + 300
        assert_eq!(answer["prime_cost"], json!("6500.000000"));
        // 6,500 + 1,500 + 400 - 600
        assert_eq!(answer["production_cost"], json!("7800.000000"));
        assert_eq!(answer["cost_per_unit"], json!("780.000000"));
    }

    #[test]
    fn nothing_produced_leaves_the_cost_per_unit_at_nil_rather_than_dividing() {
        let graph = FakeGraph::returning(vec![json!({"id": "p-1"})]);
        let answer = generate_production_statement(
            &graph,
            &scope(),
            &json!({"product_id": "pr-1", "period": "2026-01", "direct_materials_opening": "100",
                    "direct_materials_purchases": "0", "direct_materials_closing": "0",
                    "direct_labor": "0", "direct_expenses": "0", "factory_overhead": "0",
                    "work_in_progress_opening": "0", "work_in_progress_closing": "0", "units_produced": 0}),
        )
        .expect("generated");
        assert_eq!(answer["cost_per_unit"], json!("0.000000"));
    }

    #[test]
    fn both_listings_report_their_rows_under_statements() {
        let graph = FakeGraph::returning(vec![
            json!({"id": "t-1", "company_id": "c-1"}),
            json!({"id": "t-2", "company_id": "c-2"}),
        ]);
        let answer = list_trading_statements(&graph, &scope(), &json!({"company_id": "c-2"}))
            .expect("listed");
        assert_eq!(answer["statements"].as_array().unwrap().len(), 1);
    }
}
