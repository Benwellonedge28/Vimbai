#![forbid(unsafe_code)]
//! Absorption costing product-cost card, overhead absorption and
//! cost-plus pricing, ported from absorption-costing-service
//! (calculation core only — the journal-entry side effect and Neo4j
//! persistence remain in the Python service until the seam is wired).

use serde::{Deserialize, Serialize};
use vimbai_common::py_round2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductCostOutput {
    pub product_id: String,
    pub product_name: String,
    pub period: String,
    pub direct_materials: f64,
    pub direct_labor: f64,
    pub direct_expenses: f64,
    pub manufacturing_overhead: f64,
    pub units_produced: f64,
    pub opening_stock: f64,
    pub closing_stock: f64,
    pub prime_cost: f64,
    pub total_production_cost: f64,
    pub cost_per_unit: f64,
}

// Mirrors the FastAPI endpoint signature (10 explicit params); kept
// flat for the future Python seam.
#[allow(clippy::too_many_arguments)]
/// Absorption product cost card
/// (absorption-costing-service `/product-costs/calculate`):
/// prime = DM + DL + DE; total = prime + overhead;
/// cost_per_unit = total / units (0 when units <= 0).
pub fn product_cost(
    product_id: &str,
    product_name: &str,
    period: &str,
    direct_materials: f64,
    direct_labor: f64,
    direct_expenses: f64,
    manufacturing_overhead: f64,
    units_produced: f64,
    opening_stock: f64,
    closing_stock: f64,
) -> ProductCostOutput {
    let prime_cost = direct_materials + direct_labor + direct_expenses;
    let total_production_cost = prime_cost + manufacturing_overhead;
    let cost_per_unit = if units_produced > 0.0 {
        total_production_cost / units_produced
    } else {
        0.0
    };
    ProductCostOutput {
        product_id: product_id.to_string(),
        product_name: product_name.to_string(),
        period: period.to_string(),
        direct_materials,
        direct_labor,
        direct_expenses,
        manufacturing_overhead,
        units_produced,
        opening_stock,
        closing_stock,
        prime_cost,
        total_production_cost,
        cost_per_unit: py_round2(cost_per_unit),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverheadAbsorptionOutput {
    pub product_id: String,
    pub period: String,
    pub overhead_cost: f64,
    pub absorption_base: String,
    pub absorption_base_units: f64,
    pub overhead_absorption_rate: f64,
    pub absorbed_overhead: f64,
}

/// Overhead absorption
/// (absorption-costing-service `/overhead/absorption`):
/// rate = overhead / base_units; absorbed = rate * base_units (0 when
/// base_units <= 0).
pub fn overhead_absorption(
    product_id: &str,
    period: &str,
    overhead_cost: f64,
    absorption_base: &str,
    absorption_base_units: f64,
) -> OverheadAbsorptionOutput {
    let rate = if absorption_base_units > 0.0 {
        overhead_cost / absorption_base_units
    } else {
        0.0
    };
    OverheadAbsorptionOutput {
        product_id: product_id.to_string(),
        period: period.to_string(),
        overhead_cost,
        absorption_base: absorption_base.to_string(),
        absorption_base_units,
        overhead_absorption_rate: rate,
        absorbed_overhead: rate * absorption_base_units,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostPlusOutput {
    pub product_cost: f64,
    pub markup_percentage: f64,
    pub markup_amount: f64,
    pub selling_price: f64,
}

/// Cost-plus pricing (absorption-costing-service `/cost-plus`).
pub fn cost_plus_pricing(product_cost: f64, markup_percentage: f64) -> CostPlusOutput {
    let markup_amount = product_cost * (markup_percentage / 100.0);
    CostPlusOutput {
        product_cost,
        markup_percentage,
        markup_amount,
        selling_price: product_cost + markup_amount,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn product_cost_card() {
        let c = product_cost(
            "p1", "Widget", "2026-09", 5000.0, 3000.0, 500.0, 1500.0, 1000.0, 100.0, 50.0,
        );
        assert_eq!(c.prime_cost, 8500.0);
        assert_eq!(c.total_production_cost, 10000.0);
        assert_eq!(c.cost_per_unit, 10.0);
        let zero = product_cost("p", "X", "p", 100.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        assert_eq!(zero.cost_per_unit, 0.0);
    }

    #[test]
    fn overhead_absorption_calc() {
        let a = overhead_absorption("p1", "2026-09", 15000.0, "machine_hours", 5000.0);
        assert_eq!(a.overhead_absorption_rate, 3.0);
        assert_eq!(a.absorbed_overhead, 15000.0);
        let zero = overhead_absorption("p", "p", 100.0, "u", 0.0);
        assert_eq!(zero.overhead_absorption_rate, 0.0);
        assert_eq!(zero.absorbed_overhead, 0.0);
    }

    #[test]
    fn cost_plus_pricing_calc() {
        let p = cost_plus_pricing(250.0, 20.0);
        assert_eq!(p.markup_amount, 50.0);
        assert_eq!(p.selling_price, 300.0);
    }
}
