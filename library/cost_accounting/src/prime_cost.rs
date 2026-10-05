#![forbid(unsafe_code)]
//! Prime cost, ported from prime-costing-service... actually
//! prime-cost-service. Storage-free: item aggregation takes a slice.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectCostItem {
    pub item_name: String,
    pub item_type: String,
    pub amount: f64,
    pub units: f64,
    pub cost_per_unit: f64,
    pub product_id: Option<String>,
}

/// Register a direct cost item (prime-cost-service `/direct-costs/add`).
pub fn direct_cost_item(
    item_name: &str,
    item_type: &str,
    amount: f64,
    units: f64,
    product_id: Option<&str>,
) -> DirectCostItem {
    DirectCostItem {
        item_name: item_name.to_string(),
        item_type: item_type.to_string(),
        amount,
        units,
        cost_per_unit: if units > 0.0 { amount / units } else { 0.0 },
        product_id: product_id.map(str::to_string),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrimeCostCalculation {
    pub product_id: String,
    pub period: String,
    pub direct_materials: f64,
    pub direct_labor: f64,
    pub direct_expenses: f64,
    pub prime_cost: f64,
    pub cost_breakdown: BTreeMap<String, f64>,
}

fn build(product_id: &str, period: &str, dm: f64, dl: f64, de: f64) -> PrimeCostCalculation {
    let prime_cost = dm + dl + de;
    let mut cost_breakdown = BTreeMap::new();
    cost_breakdown.insert("direct_materials".to_string(), dm);
    cost_breakdown.insert("direct_labor".to_string(), dl);
    cost_breakdown.insert("direct_expenses".to_string(), de);
    cost_breakdown.insert("prime_cost".to_string(), prime_cost);
    PrimeCostCalculation {
        product_id: product_id.to_string(),
        period: period.to_string(),
        direct_materials: dm,
        direct_labor: dl,
        direct_expenses: de,
        prime_cost,
        cost_breakdown,
    }
}

/// Prime cost from explicit components
/// (prime-cost-service `/calculate`).
pub fn prime_cost(
    product_id: &str,
    period: &str,
    direct_materials: f64,
    direct_labor: f64,
    direct_expenses: f64,
) -> PrimeCostCalculation {
    build(
        product_id,
        period,
        direct_materials,
        direct_labor,
        direct_expenses,
    )
}

/// Prime cost aggregated from the caller's direct cost items
/// (prime-cost-service `/calculate-with-items`). Items are matched by
/// `product_id` and summed per `item_type`, mirroring the Python loop.
pub fn prime_cost_from_items(
    product_id: &str,
    period: &str,
    items: &[DirectCostItem],
) -> PrimeCostCalculation {
    let (mut dm, mut dl, mut de) = (0.0, 0.0, 0.0);
    for i in items
        .iter()
        .filter(|i| i.product_id.as_deref() == Some(product_id))
    {
        match i.item_type.as_str() {
            "direct_material" => dm += i.amount,
            "direct_labor" => dl += i.amount,
            "direct_expense" => de += i.amount,
            _ => {}
        }
    }
    build(product_id, period, dm, dl, de)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prime_cost_direct() {
        let c = prime_cost("p1", "2026-09", 5000.0, 3000.0, 500.0);
        assert_eq!(c.prime_cost, 8500.0);
        assert_eq!(c.cost_breakdown["prime_cost"], 8500.0);
    }

    #[test]
    fn prime_cost_from_items_aggregates() {
        let items = vec![
            direct_cost_item("steel", "direct_material", 1000.0, 1.0, Some("p1")),
            direct_cost_item("bolts", "direct_material", 200.0, 1.0, Some("p1")),
            direct_cost_item("welding", "direct_labor", 800.0, 1.0, Some("p1")),
            direct_cost_item("setup", "direct_expense", 100.0, 1.0, Some("p1")),
            direct_cost_item("other product", "direct_material", 999.0, 1.0, Some("p2")),
        ];
        let c = prime_cost_from_items("p1", "2026-09", &items);
        assert_eq!(c.direct_materials, 1200.0);
        assert_eq!(c.direct_labor, 800.0);
        assert_eq!(c.direct_expenses, 100.0);
        assert_eq!(c.prime_cost, 2100.0);
    }

    #[test]
    fn cost_per_unit_guard() {
        let i = direct_cost_item("x", "direct_material", 100.0, 0.0, None);
        assert_eq!(i.cost_per_unit, 0.0);
    }
}
