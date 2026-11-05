//! Profit and margin, from revenue and costs.

use serde::{Deserialize, Serialize};

use vimbai_common::numbers::money;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProfitCalculation {
    pub revenue: f64,
    pub total_costs: f64,
    pub profit: f64,
    pub profit_margin: f64,
}

/// Profit and margin. A non-positive revenue reports a zero margin rather than
/// dividing by it.
pub fn calculate(revenue: f64, total_costs: f64) -> ProfitCalculation {
    let profit = revenue - total_costs;
    let margin = if revenue > 0.0 {
        profit / revenue * 100.0
    } else {
        0.0
    };
    // The service returned these unrounded, and margin is a ratio rather than an
    // amount, so no rounding is applied: 28.000000000000004 stays as it is.
    ProfitCalculation {
        revenue,
        total_costs,
        profit,
        profit_margin: margin,
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CashFlowProfit {
    pub cash_inflows: f64,
    pub cash_outflows: f64,
    pub net_cash_flow: f64,
    pub note: String,
}

/// Net cash movement, carrying the service's own warning that cash is not profit.
pub fn from_cash_flow(cash_inflows: f64, cash_outflows: f64) -> CashFlowProfit {
    CashFlowProfit {
        cash_inflows: money(cash_inflows),
        cash_outflows: money(cash_outflows),
        net_cash_flow: money(cash_inflows - cash_outflows),
        note: "This represents accounting profit (not cash profit)".to_string(),
    }
}

/// Mean of the years supplied; an empty list is zero, not an error.
pub fn average_profit(annual_profits: &[f64]) -> f64 {
    if annual_profits.is_empty() {
        0.0
    } else {
        annual_profits.iter().sum::<f64>() / annual_profits.len() as f64
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AverageAccountingProfit {
    pub total_revenues: f64,
    pub total_costs: f64,
    pub years: i32,
    pub average_annual_profit: f64,
}

/// Average accounting profit over a period, with the margin on the same basis.
pub fn average_accounting_profit(
    total_revenues: f64,
    total_costs: f64,
    years: i32,
) -> Option<AverageAccountingProfit> {
    if years <= 0 {
        return None;
    }
    let annual = (total_revenues - total_costs) / years as f64;
    Some(AverageAccountingProfit {
        total_revenues,
        total_costs,
        years,
        average_annual_profit: annual,
    })
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NetProfitAfterTax {
    pub revenue: f64,
    pub costs: f64,
    pub gross_profit: f64,
    pub tax_rate: f64,
    pub tax_amount: f64,
    pub net_profit_after_tax: f64,
}

/// Net profit after tax. A non-positive rate charges nothing rather than crediting a refund.
/// Figures are returned unrounded because that is what the service reports.
pub fn net_profit_after_tax(revenue: f64, costs: f64, tax_rate: f64) -> NetProfitAfterTax {
    let gross = revenue - costs;
    let tax = if tax_rate > 0.0 {
        gross * (tax_rate / 100.0)
    } else {
        0.0
    };
    NetProfitAfterTax {
        revenue,
        costs,
        gross_profit: gross,
        tax_rate,
        tax_amount: tax,
        net_profit_after_tax: gross - tax,
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProjectProfit {
    pub revenue: f64,
    pub costs: f64,
    pub profit: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProfitComparison {
    pub project_a: ProjectProfit,
    pub project_b: ProjectProfit,
    pub difference: f64,
    pub recommendation: String,
}

/// Two projects side by side. A tie recommends neither, and says so.
pub fn compare_profits(
    project_a_revenue: f64,
    project_a_costs: f64,
    project_b_revenue: f64,
    project_b_costs: f64,
) -> ProfitComparison {
    let profit_a = project_a_revenue - project_a_costs;
    let profit_b = project_b_revenue - project_b_costs;
    let recommendation = if profit_a > profit_b {
        "Project A"
    } else if profit_b > profit_a {
        "Project B"
    } else {
        "Equal"
    };
    ProfitComparison {
        project_a: ProjectProfit {
            revenue: project_a_revenue,
            costs: project_a_costs,
            profit: profit_a,
        },
        project_b: ProjectProfit {
            revenue: project_b_revenue,
            costs: project_b_costs,
            profit: profit_b,
        },
        difference: profit_a - profit_b,
        recommendation: recommendation.to_string(),
    }
}
