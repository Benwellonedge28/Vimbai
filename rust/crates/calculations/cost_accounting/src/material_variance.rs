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

pub fn material_cost_variance(input: &MaterialInput) -> MaterialVariance {
    let standard_cost = input.standard_price * input.standard_quantity;
    let actual_cost = input.actual_price * input.actual_quantity;
    let total_variance = standard_cost - actual_cost;
    let price_variance = (input.standard_price - input.actual_price) * input.actual_quantity;
    let usage_variance = (input.standard_quantity - input.actual_quantity) * input.standard_price;

    MaterialVariance {
        standard_price: input.standard_price,
        actual_price: input.actual_price,
        standard_quantity: input.standard_quantity,
        actual_quantity: input.actual_quantity,
        standard_cost,
        actual_cost,
        total_material_cost_variance: py_round2(total_variance),
        price_variance: py_round2(price_variance),
        usage_variance: py_round2(usage_variance),
        total_check: py_round2(price_variance + usage_variance),
        overall: if total_variance > 0.0 {
            "Favorable".to_string()
        } else if total_variance < 0.0 {
            "Adverse".to_string()
        } else {
            "None".to_string()
        },
    }
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

/// `/multi-material`: one line per material, unrounded at the line's cost and
/// rounded only on the variance, as the service does.
pub fn multi_material_variance(materials: &[MaterialInput]) -> MultiMaterialVariance {
    let mut total_std = 0.0;
    let mut total_actual = 0.0;
    let mut lines = Vec::with_capacity(materials.len());

    for m in materials {
        let std = m.standard_price * m.standard_quantity;
        let act = m.actual_price * m.actual_quantity;
        total_std += std;
        total_actual += act;
        lines.push(MaterialLine {
            material: if m.name.is_empty() {
                "Unknown".to_string()
            } else {
                m.name.clone()
            },
            standard_cost: std,
            actual_cost: act,
            variance: py_round2(std - act),
        });
    }

    MultiMaterialVariance {
        materials: lines,
        total_standard_cost: total_std,
        total_actual_cost: total_actual,
        total_variance: py_round2(total_std - total_actual),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> MaterialInput {
        MaterialInput {
            name: "steel".to_string(),
            standard_price: 10.0,
            actual_price: 11.0,
            standard_quantity: 100.0,
            actual_quantity: 110.0,
        }
    }

    #[test]
    fn the_split_adds_back_to_the_total() {
        let v = material_cost_variance(&input());
        assert_eq!(v.standard_cost, 1000.0);
        assert_eq!(v.actual_cost, 1210.0);
        assert_eq!(v.total_material_cost_variance, -210.0);
        assert_eq!(v.price_variance, -110.0);
        assert_eq!(v.usage_variance, -100.0);
        assert_eq!(v.total_check, v.total_material_cost_variance);
        assert_eq!(v.overall, "Adverse");
    }

    #[test]
    fn spending_less_than_standard_is_favourable() {
        let v = material_cost_variance(&MaterialInput {
            standard_price: 10.0,
            actual_price: 9.0,
            standard_quantity: 100.0,
            actual_quantity: 90.0,
            ..Default::default()
        });
        assert_eq!(v.total_material_cost_variance, 190.0);
        assert_eq!(v.overall, "Favorable");
    }

    #[test]
    fn an_exact_match_reads_none() {
        let v = material_cost_variance(&MaterialInput {
            standard_price: 5.0,
            actual_price: 5.0,
            standard_quantity: 20.0,
            actual_quantity: 20.0,
            ..Default::default()
        });
        assert_eq!(v.total_material_cost_variance, 0.0);
        assert_eq!(v.overall, "None");
        // the simple endpoint has no third state and calls a tie adverse
        assert_eq!(simple_material_variance(100.0, 100.0).r#type, "Adverse");
    }

    #[test]
    fn the_multi_material_lines_keep_the_callers_names() {
        let result = multi_material_variance(&[
            input(),
            MaterialInput {
                name: String::new(),
                standard_price: 2.0,
                actual_price: 2.0,
                standard_quantity: 50.0,
                actual_quantity: 40.0,
            },
        ]);
        assert_eq!(result.materials[0].material, "steel");
        assert_eq!(result.materials[1].material, "Unknown");
        assert_eq!(result.total_standard_cost, 1100.0);
        assert_eq!(result.total_actual_cost, 1290.0);
        assert_eq!(result.total_variance, -190.0);
    }

    #[test]
    fn empty_material_lists_total_zero() {
        let result = multi_material_variance(&[]);
        assert_eq!(result.total_variance, 0.0);
        assert!(result.materials.is_empty());
        assert_eq!(
            material_cost_variance(&MaterialInput::default()).overall,
            "None"
        );
    }
}
