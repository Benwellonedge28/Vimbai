#![forbid(unsafe_code)]
//! Process/product costing core, ported from the twin services
//! process-costing-service and product-costing-service (both share the
//! same `crud.create_calculation` math).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostComponent {
    pub name: String,
    pub amount: f64,
    #[serde(default = "default_cost_type")]
    pub cost_type: String,
}

fn default_cost_type() -> String {
    "direct".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostCalculation {
    pub company_id: String,
    pub product_or_process: String,
    pub period: String,
    pub components: Vec<CostComponent>,
    pub quantity: i64,
    pub notes: String,
    pub total_cost: f64,
    pub unit_cost: f64,
}

/// Process/product costing
/// (process-costing-service + product-costing-service `/calculate`):
/// `total = sum(components)`; `unit = total / max(1, quantity)`.
pub fn create_calculation(
    company_id: &str,
    product_or_process: &str,
    period: &str,
    components: Vec<CostComponent>,
    quantity: i64,
    notes: &str,
) -> CostCalculation {
    let total_cost: f64 = components.iter().map(|c| c.amount).sum();
    let unit_cost = total_cost / quantity.max(1) as f64;
    CostCalculation {
        company_id: company_id.to_string(),
        product_or_process: product_or_process.to_string(),
        period: period.to_string(),
        components,
        quantity,
        notes: notes.to_string(),
        total_cost,
        unit_cost,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn totals_and_unit_cost() {
        let c = create_calculation(
            "c1",
            "Process A",
            "2026-09",
            vec![
                CostComponent {
                    name: "materials".into(),
                    amount: 400.0,
                    cost_type: "direct_materials".into(),
                },
                CostComponent {
                    name: "labour".into(),
                    amount: 300.0,
                    cost_type: "direct_labor".into(),
                },
                CostComponent {
                    name: "overhead".into(),
                    amount: 200.0,
                    cost_type: "overhead".into(),
                },
            ],
            100,
            "",
        );
        assert_eq!(c.total_cost, 900.0);
        assert_eq!(c.unit_cost, 9.0);
    }

    #[test]
    fn zero_quantity_max_one_guard() {
        let c = create_calculation("c1", "X", "p", vec![], 0, "");
        assert_eq!(c.total_cost, 0.0);
        assert_eq!(c.unit_cost, 0.0);
    }
}
