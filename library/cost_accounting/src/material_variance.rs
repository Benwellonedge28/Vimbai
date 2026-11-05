//! material-cost-variance-service: total material cost variance and its price
//! and usage split, for one material or a bill of them.
//!
//! PYTHON QUIRK: the service works standard minus actual, so a positive figure
//! is money saved, the reverse of the labour report next door. Price variance
//! is measured on the actual quantity and usage variance at the standard price,
//! which is what makes the two add back to the total.
use serde::{Deserialize, Serialize};
use vimbai_common::py_round2;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MaterialInput {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub standard_price: f64,
    #[serde(default)]
    pub actual_price: f64,
    #[serde(default)]
    pub standard_quantity: f64,
    #[serde(default)]
    pub actual_quantity: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MaterialVariance {
    pub standard_price: f64,
    pub actual_price: f64,
    pub standard_quantity: f64,
    pub actual_quantity: f64,
    pub standard_cost: f64,
    pub actual_cost: f64,
    pub total_material_cost_variance: f64,
    pub price_variance: f64,
    pub usage_variance: f64,
    /// Price plus usage, which should equal the total.
    pub total_check: f64,
    /// Favorable, Adverse, or None when the variance is exactly zero.
    pub overall: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SimpleVariance {
    pub standard_cost: f64,
    pub actual_cost: f64,
    pub variance: f64,
    /// Favorable or Adverse; a zero variance still reads Adverse.
    pub r#type: String,
}

/// `/simple`: two costs, no split.
pub fn simple_material_variance(standard_cost: f64, actual_cost: f64) -> SimpleVariance {
    let variance = standard_cost - actual_cost;
    SimpleVariance {
        standard_cost,
        actual_cost,
        variance: py_round2(variance),
        r#type: if variance > 0.0 {
            "Favorable".to_string()
        } else {
            "Adverse".to_string()
        },
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MaterialLine {
    pub material: String,
    pub standard_cost: f64,
    pub actual_cost: f64,
    pub variance: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MultiMaterialVariance {
    pub materials: Vec<MaterialLine>,
    pub total_standard_cost: f64,
    pub total_actual_cost: f64,
    pub total_variance: f64,
}
