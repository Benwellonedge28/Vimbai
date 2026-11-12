#![forbid(unsafe_code)]
//! Scenario budgeting, ported from scenario-budget-service.
//!
//! The three scenarios are not symmetric and the asymmetry is the point of
//! the original: revenue flexes by the caller's multipliers, but expenses
//! are hard-coded to 90% in the worst case and 110% in the best. The risk
//! verdict compares the profit spread against the base profit and only
//! ever says "Medium", never "High".

use serde::{Deserialize, Serialize};
use vimbai_common::py_round2;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScenarioResult {
    pub scenario: String,
    pub revenue: f64,
    pub expenses: f64,
    pub profit: f64,
    pub profit_margin: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScenarioBudgetOutput {
    pub company_id: String,
    pub worst_case: ScenarioResult,
    pub most_likely: ScenarioResult,
    pub best_case: ScenarioResult,
    pub risk_assessment: String,
}

fn result(name: &str, revenue: f64, expenses: f64) -> ScenarioResult {
    let profit = revenue - expenses;
    ScenarioResult {
        scenario: name.to_string(),
        revenue: py_round2(revenue),
        expenses: py_round2(expenses),
        profit: py_round2(profit),
        profit_margin: if revenue != 0.0 {
            py_round2(profit / revenue * 100.0)
        } else {
            0.0
        },
    }
}

/// `/analyze`
pub fn analyze_scenarios(
    company_id: &str,
    base_revenue: f64,
    base_expenses: f64,
    base_profit: f64,
    worst_case_multiplier: f64,
    best_case_multiplier: f64,
) -> ScenarioBudgetOutput {
    let worst = result(
        "Worst Case",
        base_revenue * worst_case_multiplier,
        base_expenses * 0.9,
    );
    let likely = result("Most Likely", base_revenue, base_expenses);
    let best = result(
        "Best Case",
        base_revenue * best_case_multiplier,
        base_expenses * 1.1,
    );

    let risk_assessment = if (worst.profit - best.profit).abs() > base_profit {
        "Medium risk - wide range between scenarios"
    } else {
        "Low risk"
    };

    ScenarioBudgetOutput {
        company_id: company_id.to_string(),
        worst_case: worst,
        most_likely: likely,
        best_case: best,
        risk_assessment: risk_assessment.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> ScenarioBudgetOutput {
        analyze_scenarios("acme", 1_000_000.0, 800_000.0, 200_000.0, 0.75, 1.25)
    }

    #[test]
    fn revenue_flexes_but_expenses_do_not() {
        let out = sample();
        assert_eq!(out.worst_case.revenue, 750_000.0);
        assert_eq!(out.worst_case.expenses, 720_000.0);
        assert_eq!(out.worst_case.profit, 30_000.0);
        assert_eq!(out.best_case.revenue, 1_250_000.0);
        assert_eq!(out.best_case.expenses, 880_000.0);
        assert_eq!(out.best_case.profit, 370_000.0);
    }

    #[test]
    fn most_likely_mirrors_the_base() {
        let out = sample();
        assert_eq!(out.most_likely.revenue, 1_000_000.0);
        assert_eq!(out.most_likely.expenses, 800_000.0);
        assert_eq!(out.most_likely.profit, 200_000.0);
        assert_eq!(out.most_likely.profit_margin, 20.0);
    }

    #[test]
    fn margins_are_percentages_rounded_to_two_places() {
        let out = analyze_scenarios("acme", 300_000.0, 200_000.0, 100_000.0, 0.8, 1.2);
        assert_eq!(out.worst_case.profit_margin, 25.0);
        assert_eq!(out.most_likely.profit_margin, 33.33);
        assert_eq!(out.best_case.profit_margin, 38.89);
    }

    #[test]
    fn zero_revenue_never_divides() {
        let out = analyze_scenarios("acme", 0.0, 50_000.0, 0.0, 0.9, 1.1);
        assert_eq!(out.worst_case.profit_margin, 0.0);
        assert_eq!(out.best_case.profit_margin, 0.0);
    }

    #[test]
    fn wide_spread_is_medium_risk() {
        // 30k worst against 370k best is a 340k spread over a 200k base
        assert_eq!(
            sample().risk_assessment,
            "Medium risk - wide range between scenarios"
        );
    }

    #[test]
    fn narrow_spread_is_low_risk() {
        let out = analyze_scenarios("acme", 1_000_000.0, 900_000.0, 900_000.0, 0.99, 1.01);
        assert_eq!(out.risk_assessment, "Low risk");
    }
}
