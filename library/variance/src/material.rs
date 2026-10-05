#![forbid(unsafe_code)]
//! Material variances, ported from material-price-variance-service,
//! material-usage-variance-service, material-cost-variance-service and
//! variance-service `/material-variance`.
//!
//! Formulas (parity with the Python services):
//!
//! 1. Price Variance = (Standard Price - Actual Price) x Actual Quantity
//! 2. Usage Variance = (Standard Quantity - Actual Quantity) x Standard Price
//! 3. Total Material Variance = (SP x SQ) - (AP x AQ)
//! 4. Efficiency = (std_qty_per_unit x units_produced - actual) x standard_price

use serde::{Deserialize, Serialize};
use vimbai_common::{favorable_adverse, py_round2};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaterialVarianceOutput {
    pub standard_price: f64,
    pub actual_price: f64,
    pub standard_quantity: f64,
    pub actual_quantity: f64,
    pub standard_cost: f64,
    pub actual_cost: f64,
    pub total_material_variance: f64,
    pub price_variance: f64,
    pub usage_variance: f64,
    #[serde(default)]
    pub price_variance_type: Option<&'static str>,
    #[serde(default)]
    pub usage_variance_type: Option<&'static str>,
}

/// Price variance (material-price service): `(SP - AP) x AQ`.
pub fn material_price_variance(
    standard_price: f64,
    actual_price: f64,
    actual_quantity: f64,
) -> f64 {
    py_round2((standard_price - actual_price) * actual_quantity)
}

/// Usage variance (material-usage service): `(SQ - AQ) x SP`.
pub fn material_usage_variance(
    standard_quantity: f64,
    actual_quantity: f64,
    standard_price: f64,
) -> f64 {
    py_round2((standard_quantity - actual_quantity) * standard_price)
}

/// Combined price/usage/total breakdown (material-cost, material-usage
/// `/total-material-variance` and variance-service `/material-variance`).
pub fn material_cost_variance(
    standard_price: f64,
    actual_price: f64,
    standard_quantity: f64,
    actual_quantity: f64,
) -> MaterialVarianceOutput {
    let standard_cost = standard_price * standard_quantity;
    let actual_cost = actual_price * actual_quantity;
    let total_variance = standard_cost - actual_cost;
    let price_variance = (standard_price - actual_price) * actual_quantity;
    let usage_variance = (standard_quantity - actual_quantity) * standard_price;

    MaterialVarianceOutput {
        standard_price,
        actual_price,
        standard_quantity,
        actual_quantity,
        standard_cost,
        actual_cost,
        total_material_variance: py_round2(total_variance),
        price_variance: py_round2(price_variance),
        usage_variance: py_round2(usage_variance),
        price_variance_type: Some(favorable_adverse(price_variance)),
        usage_variance_type: Some(favorable_adverse(usage_variance)),
    }
}

/// Total material variance only (material-cost `/simple`):
/// `standard_cost - actual_cost`.
pub fn material_total_variance(standard_cost: f64, actual_cost: f64) -> f64 {
    py_round2(standard_cost - actual_cost)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaterialEfficiencyOutput {
    pub standard_quantity_per_unit: f64,
    pub units_produced: f64,
    pub standard_quantity_total: f64,
    pub actual_quantity: f64,
    pub standard_price: f64,
    pub efficiency_variance: f64,
    #[serde(rename = "type")]
    pub variance_type: &'static str,
}

/// Material efficiency (material-usage `/calculate-material-efficiency`).
pub fn material_efficiency_variance(
    standard_quantity_per_unit: f64,
    units_produced: f64,
    actual_total_quantity: f64,
    standard_price: f64,
) -> MaterialEfficiencyOutput {
    let std_qty_total = standard_quantity_per_unit * units_produced;
    let variance = (std_qty_total - actual_total_quantity) * standard_price;
    MaterialEfficiencyOutput {
        standard_quantity_per_unit,
        units_produced,
        standard_quantity_total: std_qty_total,
        actual_quantity: actual_total_quantity,
        standard_price,
        efficiency_variance: py_round2(variance),
        variance_type: favorable_adverse(variance),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaterialLine {
    pub name: String,
    pub standard_cost: f64,
    pub actual_cost: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiMaterialOutput {
    pub materials: Vec<MaterialLine>,
    pub total_standard_cost: f64,
    pub total_actual_cost: f64,
    pub total_variance: f64,
}

/// Multi-material total variance (material-cost `/multi`).
pub fn multi_material_variance(materials: &[MaterialLine]) -> MultiMaterialOutput {
    let total_std: f64 = materials.iter().map(|m| m.standard_cost).sum();
    let total_actual: f64 = materials.iter().map(|m| m.actual_cost).sum();
    let total_variance = total_std - total_actual;
    MultiMaterialOutput {
        materials: materials.to_vec(),
        total_standard_cost: py_round2(total_std),
        total_actual_cost: py_round2(total_actual),
        total_variance: py_round2(total_variance),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn price_variance_matches_python() {
        // python: (10 - 12) * 500 = -1000
        assert_eq!(material_price_variance(10.0, 12.0, 500.0), -1000.0);
    }

    #[test]
    fn usage_variance_matches_python() {
        // python: (200 - 210) * 5 = -50
        assert_eq!(material_usage_variance(200.0, 210.0, 5.0), -50.0);
    }

    #[test]
    fn total_breakdown_matches_variance_service() {
        // python variance-service /material-variance SP=10 AP=12 SQ=100 AQ=90:
        // std_cost=1000, actual=1080, total=-80, price=-180, usage=100
        let out = material_cost_variance(10.0, 12.0, 100.0, 90.0);
        assert_eq!(out.standard_cost, 1000.0);
        assert_eq!(out.actual_cost, 1080.0);
        assert_eq!(out.total_material_variance, -80.0);
        assert_eq!(out.price_variance, -180.0);
        assert_eq!(out.usage_variance, 100.0);
        assert_eq!(out.price_variance_type.unwrap(), "Adverse");
        assert_eq!(out.usage_variance_type.unwrap(), "Favorable");
    }

    #[test]
    fn efficiency_variance_flexes_to_units() {
        // std_qty = 2*100 = 200; (200 - 190) * 4 = 40
        let out = material_efficiency_variance(2.0, 100.0, 190.0, 4.0);
        assert_eq!(out.efficiency_variance, 40.0);
        assert_eq!(out.variance_type, "Favorable");
    }

    #[test]
    fn multi_material_sums() {
        let mats = vec![
            MaterialLine {
                name: "A".into(),
                standard_cost: 100.0,
                actual_cost: 120.0,
            },
            MaterialLine {
                name: "B".into(),
                standard_cost: 300.0,
                actual_cost: 280.0,
            },
        ];
        let out = multi_material_variance(&mats);
        assert_eq!(out.total_standard_cost, 400.0);
        assert_eq!(out.total_actual_cost, 400.0);
        assert_eq!(out.total_variance, 0.0);
    }
}
