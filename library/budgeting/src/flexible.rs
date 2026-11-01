#![forbid(unsafe_code)]
//! Flexible budgets, ported from flexible-budget-service
//! (`/calculate`, `/multi-level`, `/variance-analysis`,
//! `/cost-per-unit`, `/prepare-flexed`).

use serde::{Deserialize, Serialize};
use vimbai_common::py_round2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlexedBudget {
    pub budgeted_cost: f64,
    pub budgeted_activity: f64,
    pub actual_activity: f64,
    pub flexed_budget: f64,
    pub budget_formula: String,
}

/// Simple flex (flexible-budget-service `/calculate`). A zero budgeted
/// activity is an `Err` (Python returns an error object).
pub fn flex_budget(
    budgeted_cost: f64,
    budgeted_activity: f64,
    actual_activity: f64,
) -> Result<FlexedBudget, String> {
    if budgeted_activity == 0.0 {
        return Err("Budgeted activity cannot be zero".to_string());
    }
    Ok(FlexedBudget {
        budgeted_cost,
        budgeted_activity,
        actual_activity,
        flexed_budget: (budgeted_cost / budgeted_activity) * actual_activity,
        budget_formula: format!("({budgeted_cost} / {budgeted_activity}) × {actual_activity}"),
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlexMultiLevel {
    pub budgeted_fixed_cost: f64,
    pub budgeted_variable_rate: f64,
    pub budgeted_activity: f64,
    pub actual_activity: f64,
    pub flexed_fixed_cost: f64,
    pub flexed_variable_cost: f64,
    pub flexed_total_budget: f64,
    pub original_budget: f64,
    pub spending_variance: f64,
    pub activity_variance: f64,
}

/// Fixed + variable flex (flexible-budget-service `/multi-level`).
/// `spending_variance` and `activity_variance` are exact negatives.
pub fn flex_multi_level(
    budgeted_fixed_cost: f64,
    budgeted_variable_rate: f64,
    budgeted_activity: f64,
    actual_activity: f64,
) -> FlexMultiLevel {
    let flexed_fixed = budgeted_fixed_cost;
    let flexed_variable = budgeted_variable_rate * actual_activity;
    let flexed_total = flexed_fixed + flexed_variable;
    let original_total = budgeted_fixed_cost + budgeted_variable_rate * budgeted_activity;
    FlexMultiLevel {
        budgeted_fixed_cost,
        budgeted_variable_rate,
        budgeted_activity,
        actual_activity,
        flexed_fixed_cost: flexed_fixed,
        flexed_variable_cost: flexed_variable,
        flexed_total_budget: flexed_total,
        original_budget: original_total,
        spending_variance: original_total - flexed_total,
        activity_variance: flexed_total - original_total,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlexVariance {
    pub static_budget: f64,
    pub flexed_budget: f64,
    pub actual_result: f64,
    pub sales_volume_variance: f64,
    pub expenditure_variance: f64,
    pub total_variance: f64,
    pub interpretation: String,
}

/// Full flexible-budget variance (flexible-budget-service
/// `/variance-analysis`).
pub fn flex_variance_analysis(
    static_budget: f64,
    flexed_budget: f64,
    actual_result: f64,
) -> FlexVariance {
    let sales_volume_variance = flexed_budget - static_budget;
    let expenditure_variance = flexed_budget - actual_result;
    let total_variance = static_budget - actual_result;
    let interpretation = if total_variance > 0.0 {
        "Favorable".to_string()
    } else if total_variance < 0.0 {
        "Adverse".to_string()
    } else {
        "None".to_string()
    };
    FlexVariance {
        static_budget,
        flexed_budget,
        actual_result,
        sales_volume_variance: py_round2(sales_volume_variance),
        expenditure_variance: py_round2(expenditure_variance),
        total_variance: py_round2(total_variance),
        interpretation,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlexedCostPerUnit {
    pub flexed_budget: f64,
    pub actual_activity: f64,
    pub flexed_cost_per_unit: f64,
}

/// Flexed cost per unit (flexible-budget-service `/cost-per-unit`).
pub fn flexed_cost_per_unit(flexed_budget: f64, actual_activity: f64) -> FlexedCostPerUnit {
    FlexedCostPerUnit {
        flexed_budget,
        actual_activity,
        flexed_cost_per_unit: if actual_activity > 0.0 {
            py_round2(flexed_budget / actual_activity)
        } else {
            0.0
        },
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlexReason {
    /// Activity above plan
    Favorable,
    /// Activity below plan
    Unfavorable,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlexedPrepared {
    pub original_budget: f64,
    pub original_output: f64,
    pub actual_output: f64,
    pub flexed_budget: f64,
    pub volume_variance: f64,
    pub variance_type: String,
}

/// Prepare a flexed budget with a cost/revenue direction flag
/// (flexible-budget-service `/prepare-flexed`).
/// `is_cost=true`: extra output raises the flexed budget (unfavorable);
/// `is_cost=false`: extra revenue output is favorable.
pub fn prepare_flexed(
    original_budget: f64,
    original_output: f64,
    actual_output: f64,
    is_cost: bool,
) -> FlexedPrepared {
    let flexed = if original_output > 0.0 {
        (original_budget / original_output) * actual_output
    } else {
        0.0
    };
    let volume_variance = flexed - original_budget;
    let variance_type = if is_cost {
        if volume_variance > 0.0 {
            "Adverse"
        } else if volume_variance < 0.0 {
            "Favorable"
        } else {
            "None"
        }
    } else if volume_variance > 0.0 {
        "Favorable"
    } else if volume_variance < 0.0 {
        "Adverse"
    } else {
        "None"
    }
    .to_string();
    FlexedPrepared {
        original_budget,
        original_output,
        actual_output,
        flexed_budget: py_round2(flexed),
        volume_variance: py_round2(volume_variance),
        variance_type,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_flex() {
        let f = flex_budget(10000.0, 1000.0, 1200.0).unwrap();
        assert_eq!(f.flexed_budget, 12000.0);
        assert!(f.budget_formula.contains("10000"));
        assert!(flex_budget(1000.0, 0.0, 100.0).is_err());
    }

    #[test]
    fn multi_level_flex() {
        let m = flex_multi_level(5000.0, 4.0, 1000.0, 1200.0);
        assert_eq!(m.flexed_fixed_cost, 5000.0);
        assert_eq!(m.flexed_variable_cost, 4800.0);
        assert_eq!(m.flexed_total_budget, 9800.0);
        assert_eq!(m.original_budget, 9000.0);
        assert_eq!(m.spending_variance, -800.0);
        assert_eq!(m.activity_variance, 800.0);
    }

    #[test]
    fn variance_directions() {
        let v = flex_variance_analysis(9000.0, 9800.0, 9500.0);
        assert_eq!(v.sales_volume_variance, 800.0);
        assert_eq!(v.expenditure_variance, 300.0);
        assert_eq!(v.total_variance, -500.0);
        assert_eq!(v.interpretation, "Adverse");
        assert_eq!(
            flex_variance_analysis(100.0, 100.0, 90.0).interpretation,
            "Favorable"
        );
        assert_eq!(
            flex_variance_analysis(100.0, 100.0, 100.0).interpretation,
            "None"
        );
    }

    #[test]
    fn cost_per_unit_guard() {
        assert_eq!(
            flexed_cost_per_unit(9800.0, 1200.0).flexed_cost_per_unit,
            8.17
        );
        assert_eq!(flexed_cost_per_unit(9800.0, 0.0).flexed_cost_per_unit, 0.0);
    }

    #[test]
    fn prepared_flexed_directions() {
        // Cost: higher output -> unfavorable
        let c = prepare_flexed(10000.0, 1000.0, 1200.0, true);
        assert_eq!(c.flexed_budget, 12000.0);
        assert_eq!(c.variance_type, "Adverse");
        // Revenue: higher output -> favorable
        let r = prepare_flexed(10000.0, 1000.0, 1200.0, false);
        assert_eq!(r.variance_type, "Favorable");
        // No movement
        assert_eq!(
            prepare_flexed(10000.0, 1000.0, 1000.0, true).variance_type,
            "None"
        );
        // Zero denominator
        let z = prepare_flexed(10000.0, 0.0, 500.0, true);
        assert_eq!(z.flexed_budget, 0.0);
        assert_eq!(z.volume_variance, -10000.0);
        assert_eq!(z.variance_type, "Favorable");
    }
}
