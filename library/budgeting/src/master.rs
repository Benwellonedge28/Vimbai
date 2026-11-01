#![forbid(unsafe_code)]
//! Master budget, ported from master-budget-service `/prepare`.
//! The budgeted balance sheet is a fixed placeholder in the Python
//! service and is preserved verbatim.

use serde::{Deserialize, Serialize};

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
