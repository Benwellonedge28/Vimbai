#![forbid(unsafe_code)]
//! Standard costing, ported from standard-cost-service.

use serde::{Deserialize, Serialize};
use vimbai_common::py_round2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaterialStandard {
    pub standard_price_per_unit: f64,
    pub standard_quantity: f64,
    pub standard_material_cost: f64,
}

/// `price * quantity` (standard-cost-service `/material-standard`).
pub fn material_standard(standard_price_per_unit: f64, standard_quantity: f64) -> MaterialStandard {
    MaterialStandard {
        standard_price_per_unit,
        standard_quantity,
        standard_material_cost: standard_price_per_unit * standard_quantity,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabourStandard {
    pub standard_rate_per_hour: f64,
    pub standard_hours: f64,
    pub standard_labour_cost: f64,
}

/// `rate * hours` (standard-cost-service `/labour-standard`).
pub fn labour_standard(standard_rate_per_hour: f64, standard_hours: f64) -> LabourStandard {
    LabourStandard {
        standard_rate_per_hour,
        standard_hours,
        standard_labour_cost: standard_rate_per_hour * standard_hours,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverheadStandard {
    pub overhead_rate: f64,
    pub standard_activity_level: f64,
    pub standard_overhead_cost: f64,
}

/// `rate * activity` (standard-cost-service `/overhead-standard`).
pub fn overhead_standard(overhead_rate: f64, standard_activity_level: f64) -> OverheadStandard {
    OverheadStandard {
        overhead_rate,
        standard_activity_level,
        standard_overhead_cost: overhead_rate * standard_activity_level,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TotalStandard {
    pub standard_direct_material: f64,
    pub standard_direct_labour: f64,
    pub standard_overhead: f64,
    pub total_standard_cost: f64,
}

/// Sum of standard cost families
/// (standard-cost-service `/total-standard`).
pub fn total_standard(
    standard_direct_material: f64,
    standard_direct_labour: f64,
    standard_overhead: f64,
) -> TotalStandard {
    TotalStandard {
        standard_direct_material,
        standard_direct_labour,
        standard_overhead,
        total_standard_cost: standard_direct_material + standard_direct_labour + standard_overhead,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StandardPerUnit {
    pub standard_material_cost: f64,
    pub standard_labour_cost: f64,
    pub standard_overhead: f64,
    pub total_standard_cost: f64,
    pub units_produced: f64,
    pub standard_cost_per_unit: f64,
}

/// Standard cost per unit (standard-cost-service `/per-unit`):
/// `total / units` rounded to 2dp (0 when units <= 0).
pub fn standard_per_unit(
    standard_material_cost: f64,
    standard_labour_cost: f64,
    standard_overhead: f64,
    units_produced: f64,
) -> StandardPerUnit {
    let total_standard = standard_material_cost + standard_labour_cost + standard_overhead;
    let cost_per_unit = if units_produced > 0.0 {
        total_standard / units_produced
    } else {
        0.0
    };
    StandardPerUnit {
        standard_material_cost,
        standard_labour_cost,
        standard_overhead,
        total_standard_cost: total_standard,
        units_produced,
        standard_cost_per_unit: py_round2(cost_per_unit),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StandardVariance {
    pub standard_cost: f64,
    pub actual_cost: f64,
    pub variance: f64,
    pub interpretation: &'static str,
}

/// Standard vs actual variance
/// (standard-cost-service `/variance-analysis`).
pub fn variance_analysis(standard_cost: f64, actual_cost: f64) -> StandardVariance {
    let variance = standard_cost - actual_cost;
    let interpretation: &'static str = if variance > 0.0 {
        "Favorable"
    } else if variance < 0.0 {
        "Adverse"
    } else {
        "None"
    };
    StandardVariance {
        standard_cost,
        actual_cost,
        variance,
        interpretation,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn family_standards() {
        assert_eq!(material_standard(5.0, 200.0).standard_material_cost, 1000.0);
        assert_eq!(labour_standard(12.0, 50.0).standard_labour_cost, 600.0);
        assert_eq!(
            overhead_standard(3.0, 1000.0).standard_overhead_cost,
            3000.0
        );
        let t = total_standard(1000.0, 600.0, 3000.0);
        assert_eq!(t.total_standard_cost, 4600.0);
    }

    #[test]
    fn per_unit_rounded() {
        let p = standard_per_unit(1000.0, 600.0, 3000.0, 999.0);
        assert_eq!(p.standard_cost_per_unit, py_round2(4600.0 / 999.0));
        assert_eq!(
            standard_per_unit(1.0, 1.0, 1.0, 0.0).standard_cost_per_unit,
            0.0
        );
    }

    #[test]
    fn variance_interpretations() {
        assert_eq!(variance_analysis(100.0, 90.0).interpretation, "Favorable");
        assert_eq!(variance_analysis(100.0, 110.0).interpretation, "Adverse");
        assert_eq!(variance_analysis(100.0, 100.0).interpretation, "None");
    }
}
