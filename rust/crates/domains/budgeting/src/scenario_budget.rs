#![forbid(unsafe_code)]
//! Scenario budgeting, ported from scenario-budget-service `/analyze`.
//!
//! The three scenarios are not symmetric and the asymmetry is the point of the
//! original: revenue flexes by the caller's multipliers, but expenses are
//! hard-coded to 90% in the worst case and 110% in the best. The risk verdict
//! compares the profit spread against the base profit and only ever says
//! "Medium", never "High".
//!
//! One decision: the Python compared the spread against `request.base_profit`,
//! a field its own request model does not have, so `/analyze` raised on every
//! call. The base profit is now the base revenue less the base expenses, which
//! is what the comparison plainly meant.

use serde::{Deserialize, Serialize};
use vimbai_common::{py_round, Money};

#[derive(Debug, Clone, Copy, PartialEq)]
enum Rate {
    Zero,
    Given(f64),
}

impl Rate {
    fn of(value: f64) -> Self {
        if value == 0.0 {
            Rate::Zero
        } else {
            Rate::Given(value)
        }
    }

    fn money(&self) -> Money {
        match self {
            Rate::Zero => Money::zero(),
            Rate::Given(value) => {
                Money::from_f64_shortest(*value).unwrap_or_else(|_| Money::zero())
            }
        }
    }
}

fn r2(value: f64) -> f64 {
    py_round(value, 2)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScenarioBudgetRequest {
    pub company_id: String,
    pub base_revenue: Money,
    pub base_expenses: Money,
    pub worst_case_multiplier: f64,
    pub best_case_multiplier: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScenarioResult {
    pub scenario: String,
    pub revenue: Money,
    pub expenses: Money,
    pub profit: Money,
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

fn result(name: &str, revenue: Money, expenses: Money) -> ScenarioResult {
    let profit = revenue.minus(&expenses);
    let profit_margin = if revenue.is_zero() {
        0.0
    } else {
        r2(profit.over(&revenue).to_f64_lossy() * 100.0)
    };
    ScenarioResult {
        scenario: name.to_string(),
        revenue,
        expenses,
        profit,
        profit_margin,
    }
}

/// `/analyze`
pub fn analyze_scenarios(request: &ScenarioBudgetRequest) -> ScenarioBudgetOutput {
    let worst = result(
        "Worst Case",
        request
            .base_revenue
            .times(&Rate::of(request.worst_case_multiplier).money())
            .round(2),
        request.base_expenses.times(&Rate::of(0.9).money()).round(2),
    );
    let likely = result(
        "Most Likely",
        request.base_revenue.clone(),
        request.base_expenses.clone(),
    );
    let best = result(
        "Best Case",
        request
            .base_revenue
            .times(&Rate::of(request.best_case_multiplier).money())
            .round(2),
        request.base_expenses.times(&Rate::of(1.1).money()).round(2),
    );

    let base_profit = request.base_revenue.minus(&request.base_expenses);
    let spread = best.profit.minus(&worst.profit).round(2);
    let risk_assessment = if spread > base_profit {
        "Medium risk - wide range between scenarios"
    } else {
        "Low risk"
    };

    ScenarioBudgetOutput {
        company_id: request.company_id.clone(),
        worst_case: worst,
        most_likely: likely,
        best_case: best,
        risk_assessment: risk_assessment.to_string(),
    }
}

/// `scenario-budget-service /analyze`.
pub fn scenario_budget_from_json(payload: &serde_json::Value) -> Result<serde_json::Value, String> {
    let request: ScenarioBudgetRequest = serde_json::from_value(payload.clone())
        .map_err(|error| format!("invalid request: {error}"))?;
    serde_json::to_value(analyze_scenarios(&request)).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(text: &str) -> Money {
        Money::parse(text).expect("amount")
    }

    fn fixed(amount: &Money) -> String {
        amount.to_fixed(2)
    }

    fn sample() -> ScenarioBudgetOutput {
        analyze_scenarios(&ScenarioBudgetRequest {
            company_id: "acme".to_string(),
            base_revenue: m("1000000"),
            base_expenses: m("800000"),
            worst_case_multiplier: 0.75,
            best_case_multiplier: 1.25,
        })
    }

    #[test]
    fn revenue_flexes_but_expenses_do_not() {
        let out = sample();
        assert_eq!(fixed(&out.worst_case.revenue), "750000.00");
        assert_eq!(fixed(&out.worst_case.expenses), "720000.00");
        assert_eq!(fixed(&out.worst_case.profit), "30000.00");
        assert_eq!(fixed(&out.best_case.revenue), "1250000.00");
        assert_eq!(fixed(&out.best_case.expenses), "880000.00");
        assert_eq!(fixed(&out.best_case.profit), "370000.00");
    }

    #[test]
    fn most_likely_mirrors_the_base() {
        let out = sample();
        assert_eq!(fixed(&out.most_likely.revenue), "1000000.00");
        assert_eq!(fixed(&out.most_likely.expenses), "800000.00");
        assert_eq!(fixed(&out.most_likely.profit), "200000.00");
        assert_eq!(out.most_likely.profit_margin, 20.0);
    }

    #[test]
    fn margins_are_percentages_rounded_to_two_places() {
        let out = analyze_scenarios(&ScenarioBudgetRequest {
            company_id: "acme".to_string(),
            base_revenue: m("300000"),
            base_expenses: m("200000"),
            worst_case_multiplier: 0.8,
            best_case_multiplier: 1.2,
        });
        assert_eq!(out.worst_case.profit_margin, 25.0);
        assert_eq!(out.most_likely.profit_margin, 33.33);
        assert_eq!(out.best_case.profit_margin, 38.89);
    }

    #[test]
    fn zero_revenue_never_divides() {
        let out = analyze_scenarios(&ScenarioBudgetRequest {
            company_id: "acme".to_string(),
            base_revenue: m("0"),
            base_expenses: m("50000"),
            worst_case_multiplier: 0.9,
            best_case_multiplier: 1.1,
        });
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
        let out = analyze_scenarios(&ScenarioBudgetRequest {
            company_id: "acme".to_string(),
            base_revenue: m("1000000"),
            base_expenses: m("900000"),
            worst_case_multiplier: 0.99,
            best_case_multiplier: 1.01,
        });
        assert_eq!(out.risk_assessment, "Low risk");
    }

    #[test]
    fn a_budget_past_double_precision_flexes_exactly() {
        let out = analyze_scenarios(&ScenarioBudgetRequest {
            company_id: "acme".to_string(),
            base_revenue: m("123456789012345678901.55"),
            base_expenses: m("100000000000000000000.45"),
            worst_case_multiplier: 0.75,
            best_case_multiplier: 1.25,
        });
        assert_eq!(fixed(&out.worst_case.revenue), "92592591759259259176.16");
        assert_eq!(fixed(&out.most_likely.profit), "23456789012345678901.10");
        assert_eq!(fixed(&out.best_case.revenue), "154320986265432098626.94");
    }

    #[test]
    fn the_door_reads_the_published_shape() {
        let payload = serde_json::json!({
            "company_id": "acme", "base_revenue": "1000000", "base_expenses": "800000",
            "worst_case_multiplier": 0.75, "best_case_multiplier": 1.25
        });
        let out = scenario_budget_from_json(&payload).expect("door");
        assert_eq!(out["worst_case"]["profit"], "30000.00");
        assert_eq!(out["most_likely"]["profit_margin"], 20.0);

        let bad = serde_json::json!({"company_id": "acme", "base_revenue": "lots"});
        let error = scenario_budget_from_json(&bad).expect_err("refused");
        assert!(error.contains("invalid request"), "{error}");
    }
}
