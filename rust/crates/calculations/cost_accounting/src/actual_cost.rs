#![forbid(unsafe_code)]
//! Actual costs and budget comparison, ported from actual-cost-service.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActualMaterialCost {
    pub actual_quantity: f64,
    pub actual_price_per_unit: f64,
    pub actual_material_cost: f64,
}

/// `qty * price` (actual-cost-service `/material-actual`).
pub fn actual_material_cost(
    actual_quantity: f64,
    actual_price_per_unit: f64,
) -> ActualMaterialCost {
    ActualMaterialCost {
        actual_quantity,
        actual_price_per_unit,
        actual_material_cost: actual_quantity * actual_price_per_unit,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActualLabourCost {
    pub actual_hours: f64,
    pub actual_rate_per_hour: f64,
    pub actual_labour_cost: f64,
}

/// `hours * rate` (actual-cost-service `/labour-actual`).
pub fn actual_labour_cost(actual_hours: f64, actual_rate_per_hour: f64) -> ActualLabourCost {
    ActualLabourCost {
        actual_hours,
        actual_rate_per_hour,
        actual_labour_cost: actual_hours * actual_rate_per_hour,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TotalActualCost {
    pub actual_direct_material: f64,
    pub actual_direct_labour: f64,
    pub actual_overhead: f64,
    pub total_actual_cost: f64,
}

/// Sum of the three actual cost families
/// (actual-cost-service `/total-actual`).
pub fn total_actual_cost(
    actual_direct_material: f64,
    actual_direct_labour: f64,
    actual_overhead: f64,
) -> TotalActualCost {
    TotalActualCost {
        actual_direct_material,
        actual_direct_labour,
        actual_overhead,
        total_actual_cost: actual_direct_material + actual_direct_labour + actual_overhead,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetComparison {
    pub actual_cost: f64,
    pub budgeted_cost: f64,
    pub variance: f64,
    pub variance_percent: f64,
    pub status: &'static str,
}

/// Compare actual to budget (actual-cost-service `/compare-to-budget`).
pub fn compare_actual_to_budget(actual_cost: f64, budgeted_cost: f64) -> BudgetComparison {
    let variance = budgeted_cost - actual_cost;
    let status: &'static str = if variance > 0.0 {
        "Under budget"
    } else if variance < 0.0 {
        "Over budget"
    } else {
        "On budget"
    };
    BudgetComparison {
        actual_cost,
        budgeted_cost,
        variance,
        variance_percent: if budgeted_cost != 0.0 {
            (variance / budgeted_cost) * 100.0
        } else {
            0.0
        },
        status,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VarianceLine {
    pub actual: f64,
    pub budgeted: f64,
    pub variance: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VarianceBreakdown {
    pub material: VarianceLine,
    pub labour: VarianceLine,
    pub overhead: VarianceLine,
    pub total_variance: f64,
}

/// Per-category variance breakdown
/// (actual-cost-service `/variance-breakdown`).
pub fn variance_breakdown(
    actual_material: f64,
    budgeted_material: f64,
    actual_labour: f64,
    budgeted_labour: f64,
    actual_overhead: f64,
    budgeted_overhead: f64,
) -> VarianceBreakdown {
    let material = VarianceLine {
        actual: actual_material,
        budgeted: budgeted_material,
        variance: budgeted_material - actual_material,
    };
    let labour = VarianceLine {
        actual: actual_labour,
        budgeted: budgeted_labour,
        variance: budgeted_labour - actual_labour,
    };
    let overhead = VarianceLine {
        actual: actual_overhead,
        budgeted: budgeted_overhead,
        variance: budgeted_overhead - actual_overhead,
    };
    let total_variance = material.variance + labour.variance + overhead.variance;
    VarianceBreakdown {
        material,
        labour,
        overhead,
        total_variance,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_common::py_round2;

    #[test]
    fn actual_families() {
        assert_eq!(actual_material_cost(50.0, 4.0).actual_material_cost, 200.0);
        assert_eq!(actual_labour_cost(30.0, 15.0).actual_labour_cost, 450.0);
        let t = total_actual_cost(200.0, 450.0, 350.0);
        assert_eq!(t.total_actual_cost, 1000.0);
    }

    #[test]
    fn budget_comparison() {
        let under = compare_actual_to_budget(900.0, 1000.0);
        assert_eq!(under.variance, 100.0);
        assert_eq!(under.variance_percent, 10.0);
        assert_eq!(under.status, "Under budget");
        let over = compare_actual_to_budget(1100.0, 1000.0);
        assert_eq!(over.status, "Over budget");
        assert_eq!(over.variance_percent, -10.0);
        assert_eq!(compare_actual_to_budget(100.0, 100.0).status, "On budget");
        assert_eq!(compare_actual_to_budget(50.0, 0.0).variance_percent, 0.0);
    }

    #[test]
    fn breakdown_sums() {
        let b = variance_breakdown(210.0, 200.0, 440.0, 450.0, 350.0, 350.0);
        assert_eq!(b.material.variance, -10.0);
        assert_eq!(b.labour.variance, 10.0);
        assert_eq!(b.overhead.variance, 0.0);
        assert_eq!(b.total_variance, 0.0);
    }

    #[test]
    fn rounding_helper_matches_python() {
        // Verified against Python round(x, 2)
        assert_eq!(py_round2(2.345), 2.35);
        assert_eq!(py_round2(2.675), 2.67);
        assert_eq!(py_round2(1.005), 1.0);
    }
}
