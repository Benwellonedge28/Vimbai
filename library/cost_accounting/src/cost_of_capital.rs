#![forbid(unsafe_code)]
//! cost-of-capital-service: rates of interest and the weighted average cost of
//! capital, ported with the service's own arithmetic and rounding.
use serde::{Deserialize, Serialize};

/// A float as Python's `str()` writes it: an integral value keeps its `.0`.
fn py_float(value: f64) -> String {
    if value.is_finite() && value.fract() == 0.0 {
        format!("{:.1}", value)
    } else {
        format!("{}", value)
    }
}

/// Python's `f"{value}%"` for the plain decimal rate.
fn as_percentage(rate: f64) -> String {
    format!("{}%", py_float(rate * 100.0))
}

/// Python's `f"{value:.2f}%"`.
fn as_percentage_2dp(rate: f64) -> String {
    format!("{:.2}%", rate * 100.0)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostOfCapital {
    pub base_rate: f64,
    pub risk_premium: f64,
    pub total_cost_of_capital: f64,
    pub as_percentage: String,
}

pub fn calculate(interest_rate: f64, risk_premium: f64) -> CostOfCapital {
    let total = interest_rate + risk_premium;
    CostOfCapital {
        base_rate: interest_rate,
        risk_premium,
        total_cost_of_capital: total,
        as_percentage: as_percentage(total),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostOfCapitalPercent {
    pub interest_rate_percent: f64,
    pub risk_premium_percent: f64,
    pub total_rate_decimal: f64,
    pub total_rate_percent: f64,
}

pub fn from_percent(interest_rate_percent: f64, risk_premium_percent: f64) -> CostOfCapitalPercent {
    let total = interest_rate_percent + risk_premium_percent;
    CostOfCapitalPercent {
        interest_rate_percent,
        risk_premium_percent,
        total_rate_decimal: total / 100.0,
        total_rate_percent: total,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Wacc {
    pub equity_value: f64,
    pub debt_value: f64,
    pub total_capital: f64,
    pub cost_of_equity: f64,
    pub cost_of_debt: f64,
    pub after_tax_cost_debt: f64,
    pub equity_weight: f64,
    pub debt_weight: f64,
    pub wacc: f64,
    pub wacc_percent: String,
}

/// No capital at all is the service's only refusal, and it answers with an
/// `error` body rather than a status, so the shape is kept as it was.
pub fn wacc(
    equity_value: f64,
    cost_of_equity: f64,
    debt_value: f64,
    cost_of_debt: f64,
    tax_rate: f64,
) -> Result<Wacc, String> {
    let total_capital = equity_value + debt_value;
    if total_capital == 0.0 {
        return Err("Total capital cannot be zero".to_string());
    }
    let after_tax_cost_debt = if tax_rate > 0.0 {
        cost_of_debt * (1.0 - tax_rate / 100.0)
    } else {
        cost_of_debt
    };
    let rate = (equity_value * cost_of_equity + debt_value * after_tax_cost_debt) / total_capital;
    Ok(Wacc {
        equity_value,
        debt_value,
        total_capital,
        cost_of_equity,
        cost_of_debt,
        after_tax_cost_debt,
        equity_weight: equity_value / total_capital,
        debt_weight: debt_value / total_capital,
        wacc: vimbai_common::py_round(rate, 6),
        wacc_percent: as_percentage_2dp(rate),
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimpleRate {
    pub principal: f64,
    pub amount_after: f64,
    pub years: i64,
    pub simple_rate: f64,
    pub rate_percent: String,
}

pub fn simple_rate(principal: f64, amount_after: f64, years: i64) -> Result<SimpleRate, String> {
    if years == 0 || principal == 0.0 {
        return Err("Years and principal must be positive".to_string());
    }
    let rate = ((amount_after / principal) - 1.0) / years as f64;
    Ok(SimpleRate {
        principal,
        amount_after,
        years,
        simple_rate: vimbai_common::py_round(rate, 6),
        rate_percent: as_percentage_2dp(rate),
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompoundRate {
    pub present_value: f64,
    pub future_value: f64,
    pub years: i64,
    pub compound_rate: f64,
    pub rate_percent: String,
}

pub fn compound_rate(
    present_value: f64,
    future_value: f64,
    years: i64,
) -> Result<CompoundRate, String> {
    if years == 0 || present_value == 0.0 {
        return Err("Years and present value must be positive".to_string());
    }
    let rate = (future_value / present_value).powf(1.0 / years as f64) - 1.0;
    Ok(CompoundRate {
        present_value,
        future_value,
        years,
        compound_rate: vimbai_common::py_round(rate, 6),
        rate_percent: as_percentage_2dp(rate),
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StandardRate {
    pub name: String,
    pub description: String,
}

/// `/list` answers from a fixed table in the service.
pub fn standard_rates() -> Vec<StandardRate> {
    [
        ("Treasury Rate", "Risk-free rate"),
        ("Cost of Debt", "Interest rate on borrowed capital"),
        ("Cost of Equity", "Required return by shareholders"),
        ("WACC", "Weighted Average Cost of Capital"),
    ]
    .iter()
    .map(|(name, description)| StandardRate {
        name: name.to_string(),
        description: description.to_string(),
    })
    .collect()
}
