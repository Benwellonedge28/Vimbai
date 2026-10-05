#![forbid(unsafe_code)]
//! Marginal (variable) costing, ported from marginal-costing-service
//! with exact formula parity. Storage-free: the caller persists results.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarginalCostOutput {
    pub cost_name: String,
    pub cost_code: String,
    pub variable_cost: f64,
    pub fixed_cost: f64,
    pub total_cost: f64,
    pub unit_variable_cost: f64,
    pub units: f64,
    pub cost_driver: String,
}

/// Register a marginal cost item (marginal-costing-service `/marginal-costs`).
/// `total_cost = variable + fixed`; `unit_variable_cost = variable / units`.
pub fn marginal_cost_item(
    cost_name: &str,
    cost_code: &str,
    variable_cost: f64,
    fixed_cost: f64,
    units: f64,
    cost_driver: &str,
) -> MarginalCostOutput {
    MarginalCostOutput {
        cost_name: cost_name.to_string(),
        cost_code: cost_code.to_string(),
        variable_cost,
        fixed_cost,
        total_cost: variable_cost + fixed_cost,
        unit_variable_cost: if units > 0.0 {
            variable_cost / units
        } else {
            0.0
        },
        units,
        cost_driver: cost_driver.to_string(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarginalIncomeStatementOutput {
    pub company_id: String,
    pub period: String,
    pub sales_revenue: f64,
    pub total_variable_costs: f64,
    pub contribution: f64,
    pub fixed_costs: f64,
    pub profit: f64,
    pub variable_cost_breakdown: BTreeMap<String, f64>,
}

/// Marginal income statement
/// (marginal-costing-service `/income-statement/generate`).
pub fn marginal_income_statement(
    company_id: &str,
    period: &str,
    sales_revenue: f64,
    variable_costs: &BTreeMap<String, f64>,
    fixed_costs: f64,
) -> MarginalIncomeStatementOutput {
    let total_variable_costs: f64 = variable_costs.values().sum();
    let contribution = sales_revenue - total_variable_costs;
    MarginalIncomeStatementOutput {
        company_id: company_id.to_string(),
        period: period.to_string(),
        sales_revenue,
        total_variable_costs,
        contribution,
        fixed_costs,
        profit: contribution - fixed_costs,
        variable_cost_breakdown: variable_costs.clone(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContributionAnalysisOutput {
    pub product_id: String,
    pub selling_price: f64,
    pub variable_cost_per_unit: f64,
    pub contribution_per_unit: f64,
    pub contribution_margin_ratio: f64,
    pub total_contribution: f64,
    pub fixed_costs_allocated: f64,
    pub profit_from_product: f64,
}

/// Product contribution analysis
/// (marginal-costing-service `/contribution/analyze`).
pub fn contribution_analysis(
    product_id: &str,
    selling_price: f64,
    variable_cost_per_unit: f64,
    units_sold: f64,
    fixed_costs: f64,
) -> ContributionAnalysisOutput {
    let contribution_per_unit = selling_price - variable_cost_per_unit;
    let contribution_margin_ratio = if selling_price > 0.0 {
        (contribution_per_unit / selling_price) * 100.0
    } else {
        0.0
    };
    let total_contribution = contribution_per_unit * units_sold;
    ContributionAnalysisOutput {
        product_id: product_id.to_string(),
        selling_price,
        variable_cost_per_unit,
        contribution_per_unit,
        contribution_margin_ratio,
        total_contribution,
        fixed_costs_allocated: fixed_costs,
        profit_from_product: total_contribution - fixed_costs,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfitForecastOutput {
    pub selling_price: f64,
    pub variable_cost_per_unit: f64,
    pub contribution_per_unit: f64,
    pub contribution_margin_ratio: f64,
    pub expected_units: f64,
    pub total_contribution: f64,
    pub fixed_costs: f64,
    pub forecast_profit: f64,
}

/// Profit forecast (marginal-costing-service `/profit-forecast`).
pub fn forecast_profit(
    selling_price: f64,
    variable_cost_per_unit: f64,
    fixed_costs: f64,
    expected_units: f64,
) -> ProfitForecastOutput {
    let contribution_per_unit = selling_price - variable_cost_per_unit;
    let total_contribution = contribution_per_unit * expected_units;
    ProfitForecastOutput {
        selling_price,
        variable_cost_per_unit,
        contribution_per_unit,
        contribution_margin_ratio: if selling_price > 0.0 {
            contribution_per_unit / selling_price * 100.0
        } else {
            0.0
        },
        expected_units,
        total_contribution,
        fixed_costs,
        forecast_profit: total_contribution - fixed_costs,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marginal_cost_item_math() {
        let c = marginal_cost_item("labour", "LC-01", 5000.0, 2000.0, 1000.0, "hours");
        assert_eq!(c.total_cost, 7000.0);
        assert_eq!(c.unit_variable_cost, 5.0);
        let zero = marginal_cost_item("x", "X", 100.0, 0.0, 0.0, "units");
        assert_eq!(zero.unit_variable_cost, 0.0);
    }

    #[test]
    fn income_statement_math() {
        let mut vc = BTreeMap::new();
        vc.insert("materials".to_string(), 3000.0);
        vc.insert("labour".to_string(), 2000.0);
        let s = marginal_income_statement("c1", "2026-09", 10000.0, &vc, 2500.0);
        assert_eq!(s.total_variable_costs, 5000.0);
        assert_eq!(s.contribution, 5000.0);
        assert_eq!(s.profit, 2500.0);
    }

    #[test]
    fn contribution_analysis_math() {
        let a = contribution_analysis("p1", 20.0, 12.0, 400.0, 1500.0);
        assert_eq!(a.contribution_per_unit, 8.0);
        assert_eq!(a.contribution_margin_ratio, 40.0);
        assert_eq!(a.total_contribution, 3200.0);
        assert_eq!(a.profit_from_product, 1700.0);
    }

    #[test]
    fn profit_forecast_math() {
        let f = forecast_profit(25.0, 15.0, 6000.0, 1000.0);
        assert_eq!(f.contribution_per_unit, 10.0);
        assert_eq!(f.contribution_margin_ratio, 40.0);
        assert_eq!(f.total_contribution, 10000.0);
        assert_eq!(f.forecast_profit, 4000.0);
    }
}
