#![forbid(unsafe_code)]
//! Master budget, ported from master-budget-service `/prepare`.
//! The budgeted balance sheet is a fixed placeholder in the Python
//! service and is preserved verbatim.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MasterBudgetRequest {
    pub company_id: String,
    pub budget_year: String,
    pub expected_sales_units: f64,
    pub selling_price_per_unit: f64,
    pub expected_production_units: f64,
    pub direct_material_cost_per_unit: f64,
    pub direct_labour_cost_per_unit: f64,
    pub variable_overhead_rate: f64,
    pub fixed_overhead: f64,
    pub operating_expenses: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetLine {
    pub units: Option<f64>,
    pub revenue: Option<f64>,
    pub units_to_produce: Option<f64>,
    pub opening_inventory: Option<f64>,
    pub closing_inventory: Option<f64>,
    pub cost_per_unit: Option<f64>,
    pub total_cost: Option<f64>,
    pub variable: Option<f64>,
    pub fixed: Option<f64>,
    pub total: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MasterBudgetResult {
    pub company_id: String,
    pub budget_year: String,
    pub sales_units: f64,
    pub sales_revenue: f64,
    pub units_to_produce: f64,
    pub opening_inventory: f64,
    pub closing_inventory: f64,
    pub materials_total: f64,
    pub labour_total: f64,
    pub overhead_variable: f64,
    pub overhead_fixed: f64,
    pub overhead_total: f64,
    pub cost_of_goods_sold: f64,
    pub gross_profit: f64,
    pub operating_expenses: f64,
    pub operating_profit: f64,
    pub total_assets: f64,
    pub total_equity: f64,
}

/// Master budget (master-budget-service `/prepare`).
/// `operating_profit = revenue - COGS - opex` where
/// `COGS = materials + labour + variable overhead + fixed overhead`.
pub fn prepare_master_budget(req: &MasterBudgetRequest) -> MasterBudgetResult {
    let sales_revenue = req.expected_sales_units * req.selling_price_per_unit;
    let materials_total = req.expected_production_units * req.direct_material_cost_per_unit;
    let labour_total = req.expected_production_units * req.direct_labour_cost_per_unit;
    let overhead_variable = req.expected_production_units * req.variable_overhead_rate;
    let overhead_fixed = req.fixed_overhead;
    let overhead_total = overhead_variable + overhead_fixed;
    let cost_of_goods_sold = materials_total + labour_total + overhead_total;
    let gross_profit = sales_revenue - cost_of_goods_sold;
    let operating_profit = gross_profit - req.operating_expenses;
    MasterBudgetResult {
        company_id: req.company_id.clone(),
        budget_year: req.budget_year.clone(),
        sales_units: req.expected_sales_units,
        sales_revenue,
        units_to_produce: req.expected_production_units,
        // Fixed placeholders, verbatim from the Python service
        opening_inventory: 1000.0,
        closing_inventory: 1000.0,
        materials_total,
        labour_total,
        overhead_variable,
        overhead_fixed,
        overhead_total,
        cost_of_goods_sold,
        gross_profit,
        operating_expenses: req.operating_expenses,
        operating_profit,
        total_assets: 1_000_000.0,
        total_equity: 600_000.0,
    }
}

/// `master-budget-service /prepare`.
///
/// The crate keeps the figures flat; the service published them as six named statements, so the
/// adapter regroups them. Inventory openings and closings are the fixed 1000 the service assumes.
pub fn prepare_from_json(payload: &Value) -> Result<Value, String> {
    let number = |key: &str| payload.get(key).and_then(Value::as_f64).unwrap_or(0.0);
    let request = MasterBudgetRequest {
        company_id: payload
            .get("company_id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        budget_year: payload
            .get("budget_year")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        expected_sales_units: number("expected_sales_units"),
        selling_price_per_unit: number("selling_price_per_unit"),
        expected_production_units: number("expected_production_units"),
        direct_material_cost_per_unit: number("direct_material_cost_per_unit"),
        direct_labour_cost_per_unit: number("direct_labour_cost_per_unit"),
        variable_overhead_rate: number("variable_overhead_rate"),
        fixed_overhead: number("fixed_overhead"),
        operating_expenses: number("operating_expenses"),
    };
    let result = prepare_master_budget(&request);
    Ok(serde_json::json!({
        "company_id": result.company_id,
        "budget_year": result.budget_year,
        "sales_budget": {
            "units": result.sales_units,
            "revenue": result.sales_revenue,
        },
        "production_budget": {
            "units_to_produce": result.units_to_produce,
            "opening_inventory": result.opening_inventory,
            "closing_inventory": result.closing_inventory,
        },
        "direct_materials_budget": {
            "cost_per_unit": request.direct_material_cost_per_unit,
            "total_cost": result.materials_total,
        },
        "direct_labour_budget": {
            "cost_per_unit": request.direct_labour_cost_per_unit,
            "total_cost": result.labour_total,
        },
        "overhead_budget": {
            "variable": result.overhead_variable,
            "fixed": result.overhead_fixed,
            "total": result.overhead_total,
        },
        "budgeted_balance_sheet": {
            "total_assets": result.total_assets,
            "total_equity": result.total_equity,
        },
        "budgeted_cash_flow": {
            "operating": result.operating_profit,
            "financing": 0.0,
        },
        "expected_profit": result.operating_profit,
        "budgeted_income_statement": {
            "revenue": result.sales_revenue,
            "cost_of_goods_sold": result.cost_of_goods_sold,
            "gross_profit": result.gross_profit,
            "operating_expenses": result.operating_expenses,
            "operating_profit": result.operating_profit,
        },
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn master_budget_full() {
        let req = MasterBudgetRequest {
            company_id: "c1".into(),
            budget_year: "2027".into(),
            expected_sales_units: 10000.0,
            selling_price_per_unit: 50.0,
            expected_production_units: 10500.0,
            direct_material_cost_per_unit: 12.0,
            direct_labour_cost_per_unit: 8.0,
            variable_overhead_rate: 3.0,
            fixed_overhead: 50000.0,
            operating_expenses: 80000.0,
        };
        let r = prepare_master_budget(&req);
        assert_eq!(r.sales_revenue, 500000.0);
        assert_eq!(r.materials_total, 126000.0);
        assert_eq!(r.labour_total, 84000.0);
        assert_eq!(r.overhead_variable, 31500.0);
        assert_eq!(r.overhead_total, 81500.0);
        assert_eq!(r.cost_of_goods_sold, 291500.0);
        assert_eq!(r.gross_profit, 208500.0);
        assert_eq!(r.operating_profit, 128500.0);
        assert_eq!(r.total_assets, 1_000_000.0);
        assert_eq!(r.total_equity, 600_000.0);
    }
}
