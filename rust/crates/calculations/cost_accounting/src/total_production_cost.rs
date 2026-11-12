#![forbid(unsafe_code)]
//! Total production cost, ported from total-production-cost-service
//! `/calculate` (prime + factory overheads + WIP adjustment).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductionCostCalculation {
    pub product_id: String,
    pub department_id: String,
    pub period: String,
    pub direct_materials: f64,
    pub direct_labor: f64,
    pub direct_expenses: f64,
    pub factory_rent: f64,
    pub factory_depreciation: f64,
    pub factory_insurance: f64,
    pub factory_maintenance: f64,
    pub other_overhead: f64,
    pub wip_opening: f64,
    pub wip_closing: f64,
    pub units_produced: i64,
    pub prime_cost: f64,
    pub total_overhead: f64,
    pub total_production_cost: f64,
    pub cost_per_unit: f64,
}

/// Total production cost
/// (total-production-cost-service `/calculate`):
/// `total = prime + overheads + wip_opening - wip_closing`;
/// `cost_per_unit` stays 0 when no units produced.
// Mirrors the FastAPI endpoint signature (14 explicit params).
#[allow(clippy::too_many_arguments)]
pub fn calculate_total_production_cost(
    product_id: &str,
    department_id: &str,
    period: &str,
    direct_materials: f64,
    direct_labor: f64,
    direct_expenses: f64,
    factory_rent: f64,
    factory_depreciation: f64,
    factory_insurance: f64,
    factory_maintenance: f64,
    other_overhead: f64,
    wip_opening: f64,
    wip_closing: f64,
    units_produced: i64,
) -> ProductionCostCalculation {
    let prime_cost = direct_materials + direct_labor + direct_expenses;
    let total_overhead = factory_rent
        + factory_depreciation
        + factory_insurance
        + factory_maintenance
        + other_overhead;
    let total_production_cost = prime_cost + total_overhead + wip_opening - wip_closing;
    let cost_per_unit = if units_produced > 0 {
        total_production_cost / units_produced as f64
    } else {
        0.0
    };
    ProductionCostCalculation {
        product_id: product_id.to_string(),
        department_id: department_id.to_string(),
        period: period.to_string(),
        direct_materials,
        direct_labor,
        direct_expenses,
        factory_rent,
        factory_depreciation,
        factory_insurance,
        factory_maintenance,
        other_overhead,
        wip_opening,
        wip_closing,
        units_produced,
        prime_cost,
        total_overhead,
        total_production_cost,
        cost_per_unit,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_production_cost() {
        let c = calculate_total_production_cost(
            "p1", "d1", "2026-09", 50000.0, 30000.0, 2000.0, 8000.0, 4000.0, 1000.0, 2000.0, 500.0,
            3000.0, 1000.0, 5000,
        );
        assert_eq!(c.prime_cost, 82000.0);
        assert_eq!(c.total_overhead, 15500.0);
        // total = 82000 + 15500 + 3000 - 1000 = 99500
        assert_eq!(c.total_production_cost, 99500.0);
        assert_eq!(c.cost_per_unit, 19.9);
    }

    #[test]
    fn zero_units_and_wip() {
        let c = calculate_total_production_cost(
            "p1", "d1", "p", 100.0, 50.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0,
        );
        assert_eq!(c.total_production_cost, 150.0);
        assert_eq!(c.cost_per_unit, 0.0);
    }
}
