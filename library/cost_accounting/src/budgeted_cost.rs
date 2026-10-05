#![forbid(unsafe_code)]
//! Budgeted costs and flexible budgets, ported from budgeted-cost-service.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetedCost {
    pub department: String,
    pub period: String,
    pub direct_material: f64,
    pub direct_labour: f64,
    pub overhead: f64,
    pub other_costs: f64,
    pub total_budgeted_cost: f64,
}

/// Budgeted cost breakdown (budgeted-cost-service `/create`).
pub fn create_budgeted_cost(
    department: &str,
    period: &str,
    direct_material: f64,
    direct_labour: f64,
    overhead: f64,
    other_costs: f64,
) -> BudgetedCost {
    BudgetedCost {
        department: department.to_string(),
        period: period.to_string(),
        direct_material,
        direct_labour,
        overhead,
        other_costs,
        total_budgeted_cost: direct_material + direct_labour + overhead + other_costs,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlexibleBudget {
    pub budgeted_cost_per_unit: f64,
    pub budgeted_output: f64,
    pub actual_output: f64,
    pub original_budget: f64,
    pub flexed_budget: f64,
    pub volume_variance: f64,
}

/// Flex the budget to actual output
/// (budgeted-cost-service `/flexible-budget`).
pub fn flexible_budget(
    budgeted_cost_per_unit: f64,
    actual_output: f64,
    budgeted_output: f64,
) -> FlexibleBudget {
    let flexed = budgeted_cost_per_unit * actual_output;
    let original = budgeted_cost_per_unit * budgeted_output;
    FlexibleBudget {
        budgeted_cost_per_unit,
        budgeted_output,
        actual_output,
        original_budget: original,
        flexed_budget: flexed,
        volume_variance: flexed - original,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetItem {
    pub item: String,
    pub amount: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TotalBudget {
    pub budget_items: Vec<BudgetItem>,
    pub total_budgeted_cost: f64,
}

/// Sum budget items (budgeted-cost-service `/total-budget`).
/// Mirrors the Python dict defaults: missing item -> "", missing amount -> 0.
pub fn total_budget(items: &[BudgetItem]) -> TotalBudget {
    let mut total = 0.0;
    let mut out = Vec::with_capacity(items.len());
    for i in items {
        total += i.amount;
        out.push(i.clone());
    }
    TotalBudget {
        budget_items: out,
        total_budgeted_cost: total,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputComparison {
    pub budgeted_units: f64,
    pub actual_units: f64,
    pub budgeted_cost_per_unit: f64,
    pub budgeted_total_cost: f64,
    pub flexed_budget_cost: f64,
    pub output_variance: f64,
}

/// Compare budgeted vs actual output
/// (budgeted-cost-service `/compare-output`).
pub fn compare_output(
    budgeted_units: f64,
    actual_units: f64,
    budgeted_cost_per_unit: f64,
) -> OutputComparison {
    let budgeted_total = budgeted_units * budgeted_cost_per_unit;
    let flexed = actual_units * budgeted_cost_per_unit;
    OutputComparison {
        budgeted_units,
        actual_units,
        budgeted_cost_per_unit,
        budgeted_total_cost: budgeted_total,
        flexed_budget_cost: flexed,
        output_variance: budgeted_total - flexed,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostBreakdown {
    pub total_budgeted_cost: f64,
    pub material: f64,
    pub labour: f64,
    pub overhead: f64,
}

/// Split a budget by percentages
/// (budgeted-cost-service `/cost-breakdown`). Percentages must sum to
/// 100 (within 0.01) — the Python `{"error": ...}` becomes an `Err`.
pub fn cost_breakdown(
    total_budgeted_cost: f64,
    material_percent: f64,
    labour_percent: f64,
    overhead_percent: f64,
) -> Result<CostBreakdown, String> {
    let total_percent = material_percent + labour_percent + overhead_percent;
    if (total_percent - 100.0).abs() > 0.01 {
        return Err("Percentages must sum to 100".to_string());
    }
    Ok(CostBreakdown {
        total_budgeted_cost,
        material: total_budgeted_cost * (material_percent / 100.0),
        labour: total_budgeted_cost * (labour_percent / 100.0),
        overhead: total_budgeted_cost * (overhead_percent / 100.0),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_and_total() {
        let b = create_budgeted_cost("assembly", "2026-09", 1000.0, 2000.0, 500.0, 300.0);
        assert_eq!(b.total_budgeted_cost, 3800.0);
        let t = total_budget(&[
            BudgetItem {
                item: "a".into(),
                amount: 10.0,
            },
            BudgetItem {
                item: "b".into(),
                amount: 15.0,
            },
        ]);
        assert_eq!(t.total_budgeted_cost, 25.0);
        assert_eq!(total_budget(&[]).total_budgeted_cost, 0.0);
    }

    #[test]
    fn flexible_budget_math() {
        let f = flexible_budget(5.0, 1200.0, 1000.0);
        assert_eq!(f.original_budget, 5000.0);
        assert_eq!(f.flexed_budget, 6000.0);
        assert_eq!(f.volume_variance, 1000.0);
    }

    #[test]
    fn output_comparison() {
        let c = compare_output(1000.0, 900.0, 4.0);
        assert_eq!(c.budgeted_total_cost, 4000.0);
        assert_eq!(c.flexed_budget_cost, 3600.0);
        assert_eq!(c.output_variance, 400.0);
    }

    #[test]
    fn breakdown_percent_validation() {
        let ok = cost_breakdown(10000.0, 50.0, 30.0, 20.0).unwrap();
        assert_eq!(ok.material, 5000.0);
        assert_eq!(ok.labour, 3000.0);
        assert_eq!(ok.overhead, 2000.0);
        assert!(cost_breakdown(1000.0, 50.0, 30.0, 19.0).is_err());
        // 0.005 off passes (float tolerance 0.01)
        assert!(cost_breakdown(1000.0, 50.0, 30.0, 20.005).is_ok());
        // 0.01+ off fails, matching Python float arithmetic
        assert!(cost_breakdown(1000.0, 50.0, 30.0, 20.02).is_err());
    }
}
