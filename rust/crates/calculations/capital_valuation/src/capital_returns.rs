//! Cost of capital, discount factors, discounted payback and the accounting
//! rate of return: the capital-return calculators that were still Python-only.
//!
//! Several of these reply with a pre-formatted percentage string alongside the
//! number ("12.50%"), so the strings are reproduced rather than left to the
//! client. Where a service answers with an `error` object instead of raising,
//! the Rust side returns the same shape, because the gateway passes it through.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use vimbai_common::{py_round, py_round2};

// --- cost of capital --------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CostOfCapitalOutput {
    pub base_rate: f64,
    pub risk_premium: f64,
    pub total_cost_of_capital: f64,
    pub as_percentage: String,
}

/// `/calculate`: rate and premium as decimals.
pub fn cost_of_capital(interest_rate: f64, risk_premium: f64) -> CostOfCapitalOutput {
    let total = interest_rate + risk_premium;
    CostOfCapitalOutput {
        base_rate: interest_rate,
        risk_premium,
        total_cost_of_capital: total,
        // Python prints the float, so 0.12 becomes "12.0%", not "12.00%".
        as_percentage: format!("{:?}%", total * 100.0),
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CostOfCapitalPercentOutput {
    pub interest_rate_percent: f64,
    pub risk_premium_percent: f64,
    pub total_rate_decimal: f64,
    pub total_rate_percent: f64,
}

/// `/from-percent`
pub fn cost_of_capital_percent(
    interest_rate_percent: f64,
    risk_premium_percent: f64,
) -> CostOfCapitalPercentOutput {
    CostOfCapitalPercentOutput {
        interest_rate_percent,
        risk_premium_percent,
        total_rate_decimal: (interest_rate_percent + risk_premium_percent) / 100.0,
        total_rate_percent: interest_rate_percent + risk_premium_percent,
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WaccOutput {
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

/// `/wacc`: `tax_rate` arrives as a percentage, and zero means no tax relief.
/// Empty capital is reported, not raised: the service returns `{"error": ...}`
/// with a 200, and the gateway passes that through unchanged.
pub fn wacc(
    equity_value: f64,
    cost_of_equity: f64,
    debt_value: f64,
    cost_of_debt: f64,
    tax_rate: f64,
) -> Result<WaccOutput, String> {
    let total_capital = equity_value + debt_value;
    if total_capital == 0.0 {
        return Err("Total capital cannot be zero".to_string());
    }
    let after_tax_cost_debt = if tax_rate > 0.0 {
        cost_of_debt * (1.0 - tax_rate / 100.0)
    } else {
        cost_of_debt
    };
    let value = (equity_value * cost_of_equity + debt_value * after_tax_cost_debt) / total_capital;
    Ok(WaccOutput {
        equity_value,
        debt_value,
        total_capital,
        cost_of_equity,
        cost_of_debt,
        after_tax_cost_debt,
        equity_weight: equity_value / total_capital,
        debt_weight: debt_value / total_capital,
        wacc: py_round(value, 6),
        wacc_percent: format!("{:.2}%", value * 100.0),
    })
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RateOutput {
    pub principal: f64,
    pub amount_after: f64,
    pub years: i64,
    pub simple_rate: f64,
}

/// `/simple-rate`
pub fn simple_rate(principal: f64, amount_after: f64, years: i64) -> Result<RateOutput, String> {
    if years == 0 || principal == 0.0 {
        return Err("Years and principal must be positive".to_string());
    }
    let rate = ((amount_after / principal) - 1.0) / years as f64;
    Ok(RateOutput {
        principal,
        amount_after,
        years,
        simple_rate: py_round(rate, 6),
    })
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompoundRateOutput {
    pub present_value: f64,
    pub future_value: f64,
    pub years: i64,
    pub compound_rate: f64,
    pub rate_percent: String,
}

/// `/compound-rate`: CAGR.
pub fn compound_rate(
    present_value: f64,
    future_value: f64,
    years: i64,
) -> Result<CompoundRateOutput, String> {
    if years == 0 || present_value == 0.0 {
        return Err("Years and present value must be positive".to_string());
    }
    let rate = (future_value / present_value).powf(1.0 / years as f64) - 1.0;
    Ok(CompoundRateOutput {
        present_value,
        future_value,
        years,
        compound_rate: py_round(rate, 6),
        rate_percent: format!("{:.2}%", rate * 100.0),
    })
}

// --- discount factors -------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiscountFactorOutput {
    pub rate: f64,
    pub years: i64,
    pub discount_factor: f64,
}

pub fn discount_factor(rate: f64, years: i64) -> Result<DiscountFactorOutput, String> {
    if years < 0 {
        return Err("Years cannot be negative".to_string());
    }
    Ok(DiscountFactorOutput {
        rate,
        years,
        discount_factor: 1.0 / (1.0 + rate).powi(years as i32),
    })
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiscountFactorPercentOutput {
    pub rate_percent: f64,
    pub rate_decimal: f64,
    pub years: i64,
    pub discount_factor: f64,
}

pub fn discount_factor_percent(rate_percent: f64, years: i64) -> DiscountFactorPercentOutput {
    let rate_decimal = rate_percent / 100.0;
    DiscountFactorPercentOutput {
        rate_percent,
        rate_decimal,
        years,
        discount_factor: 1.0 / (1.0 + rate_decimal).powi(years as i32),
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnnuityFactorOutput {
    pub rate: f64,
    pub years: i64,
    pub annuity_factor: f64,
    pub formula: String,
}

/// `/annuity-factor`: a zero rate degenerates to the year count.
pub fn annuity_factor(rate: f64, years: i64) -> AnnuityFactorOutput {
    let pvaf = if rate == 0.0 {
        years as f64
    } else {
        (1.0 - 1.0 / (1.0 + rate).powi(years as i32)) / rate
    };
    AnnuityFactorOutput {
        rate,
        years,
        annuity_factor: pvaf,
        formula: format!("[1 - 1/(1+{rate})^{years}]/{rate} = {pvaf}"),
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnnuityFactorPercentOutput {
    pub rate_percent: f64,
    pub rate_decimal: f64,
    pub years: i64,
    pub annuity_factor: f64,
}

pub fn annuity_factor_percent(rate_percent: f64, years: i64) -> AnnuityFactorPercentOutput {
    let rate_decimal = rate_percent / 100.0;
    let pvaf = if rate_decimal == 0.0 {
        years as f64
    } else {
        (1.0 - 1.0 / (1.0 + rate_decimal).powi(years as i32)) / rate_decimal
    };
    AnnuityFactorPercentOutput {
        rate_percent,
        rate_decimal,
        years,
        annuity_factor: pvaf,
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FactorRow {
    pub year: i64,
    pub discount_factor: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FactorTableOutput {
    pub rate: f64,
    pub factors: Vec<FactorRow>,
}

pub fn discount_factor_table(base_rate: f64, years: &[i64]) -> FactorTableOutput {
    FactorTableOutput {
        rate: base_rate,
        factors: years
            .iter()
            .map(|year| FactorRow {
                year: *year,
                discount_factor: py_round(1.0 / (1.0 + base_rate).powi(*year as i32), 6),
            })
            .collect(),
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RateFactorRow {
    pub rate: f64,
    pub discount_factor: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FactorRangeOutput {
    pub years: i64,
    pub factors: Vec<RateFactorRow>,
}

/// `/table-range` walks the ladder inclusively; a non-positive step would spin
/// forever in Python, so it is refused here.
pub fn discount_factor_table_range(
    rate_start: f64,
    rate_end: f64,
    rate_step: f64,
    years: i64,
) -> Result<FactorRangeOutput, String> {
    if rate_step <= 0.0 {
        return Err("Rate step must be positive".to_string());
    }
    let mut factors = Vec::new();
    let mut rate = rate_start;
    while rate <= rate_end {
        factors.push(RateFactorRow {
            rate: py_round(rate, 4),
            discount_factor: py_round(1.0 / (1.0 + rate).powi(years as i32), 6),
        });
        rate += rate_step;
    }
    Ok(FactorRangeOutput { years, factors })
}

// --- discounted payback -----------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiscountedScheduleRow {
    pub year: usize,
    pub cash_flow: f64,
    pub discount_factor: f64,
    pub present_value: f64,
    pub cumulative_pv: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiscountedPaybackOutput {
    pub initial_investment: f64,
    pub discount_rate: f64,
    pub discounted_cash_flow_schedule: Vec<DiscountedScheduleRow>,
    pub discounted_payback_period: Option<f64>,
    pub payback_achieved_in_year: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_pv_of_cash_flows: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// `/calculate`: pays back on the first year cumulative present value covers
/// the investment, with the partial year interpolated inside that flow.
pub fn discounted_payback(
    initial_investment: f64,
    cash_flows: &[f64],
    discount_rate: f64,
) -> DiscountedPaybackOutput {
    let mut cumulative = 0.0;
    let mut schedule = Vec::new();

    for (index, cash_flow) in cash_flows.iter().enumerate() {
        let year = index + 1;
        let factor = 1.0 / (1.0 + discount_rate).powi(year as i32);
        let present_value = cash_flow * factor;
        cumulative += present_value;
        schedule.push(DiscountedScheduleRow {
            year,
            cash_flow: *cash_flow,
            discount_factor: py_round(factor, 4),
            present_value: py_round2(present_value),
            cumulative_pv: py_round2(cumulative),
        });

        if cumulative >= initial_investment {
            let previous = cumulative - present_value;
            let remaining = initial_investment - previous;
            let fraction = if present_value != 0.0 {
                remaining / present_value
            } else {
                0.0
            };
            let payback = (year as f64 - 1.0) + fraction;
            return DiscountedPaybackOutput {
                initial_investment,
                discount_rate,
                discounted_cash_flow_schedule: schedule,
                discounted_payback_period: Some(py_round2(payback)),
                payback_achieved_in_year: Some(year),
                note: Some(
                    "More conservative than simple payback as it considers time value of money"
                        .to_string(),
                ),
                total_pv_of_cash_flows: None,
                message: None,
            };
        }
    }

    DiscountedPaybackOutput {
        initial_investment,
        discount_rate,
        discounted_cash_flow_schedule: schedule,
        discounted_payback_period: None,
        payback_achieved_in_year: None,
        note: None,
        total_pv_of_cash_flows: Some(py_round2(cumulative)),
        message: Some("Discounted payback not achieved within given period".to_string()),
    }
}

// --- accounting rate of return ---------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArrOutput {
    pub initial_investment: f64,
    pub residual_value: f64,
    pub total_profits: f64,
    pub years: i64,
    pub average_annual_profit: f64,
    pub average_investment: f64,
    pub arr_percentage: f64,
    pub arr_decimal: f64,
}

pub fn accounting_rate_of_return(
    initial_investment: f64,
    residual_value: f64,
    total_profits: f64,
    years: i64,
) -> Result<ArrOutput, String> {
    if years == 0 {
        return Err("Years must be positive".to_string());
    }
    let average_annual_profit = total_profits / years as f64;
    let average_investment = (initial_investment + residual_value) / 2.0;
    let arr = if average_investment != 0.0 {
        average_annual_profit / average_investment * 100.0
    } else {
        0.0
    };
    Ok(ArrOutput {
        initial_investment,
        residual_value,
        total_profits,
        years,
        average_annual_profit: py_round2(average_annual_profit),
        average_investment: py_round2(average_investment),
        arr_percentage: py_round2(arr),
        arr_decimal: py_round(arr / 100.0, 4),
    })
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArrFromProfitsOutput {
    pub annual_profits: Vec<f64>,
    pub number_of_years: usize,
    pub total_profits: f64,
    pub average_annual_profit: f64,
    pub initial_investment: f64,
    pub residual_value: f64,
    pub average_investment: f64,
    pub arr_percentage: f64,
}

pub fn arr_from_annual_profits(
    annual_profits: &[f64],
    initial_investment: f64,
    residual_value: f64,
) -> Result<ArrFromProfitsOutput, String> {
    if annual_profits.is_empty() {
        return Err("Annual profits required".to_string());
    }
    let total: f64 = annual_profits.iter().sum();
    let average = total / annual_profits.len() as f64;
    let average_investment = (initial_investment + residual_value) / 2.0;
    let arr = if average_investment != 0.0 {
        average / average_investment * 100.0
    } else {
        0.0
    };
    Ok(ArrFromProfitsOutput {
        annual_profits: annual_profits.to_vec(),
        number_of_years: annual_profits.len(),
        total_profits: total,
        average_annual_profit: py_round2(average),
        initial_investment,
        residual_value,
        average_investment: py_round2(average_investment),
        arr_percentage: py_round2(arr),
    })
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArrProject {
    pub name: String,
    pub average_annual_profit: f64,
    pub average_capital: f64,
    pub arr_percentage: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArrComparisonOutput {
    pub project_comparison: Vec<ArrProject>,
    pub recommended_project: Option<String>,
}

/// `/compare`: projects without profits are dropped, and the ranking is by
/// ARR descending. Ties keep the order they arrived in, as Python's stable
/// sort does.
pub fn arr_comparison(projects: &[(String, f64, f64, Vec<f64>)]) -> ArrComparisonOutput {
    let mut comparison = Vec::new();
    for (name, initial, residual, profits) in projects {
        if profits.is_empty() {
            continue;
        }
        let average = profits.iter().sum::<f64>() / profits.len() as f64;
        let average_capital = (initial + residual) / 2.0;
        let arr = if average_capital != 0.0 {
            average / average_capital * 100.0
        } else {
            0.0
        };
        comparison.push(ArrProject {
            name: name.clone(),
            average_annual_profit: py_round2(average),
            average_capital: py_round2(average_capital),
            arr_percentage: py_round2(arr),
        });
    }
    comparison.sort_by(|a, b| {
        b.arr_percentage
            .partial_cmp(&a.arr_percentage)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let recommended = comparison.first().map(|p| p.name.clone());
    ArrComparisonOutput {
        project_comparison: comparison,
        recommended_project: recommended,
    }
}

/// `/decision`
pub fn arr_decision(arr: f64, target_return: f64) -> Value {
    let decision = if arr > target_return {
        "Accept"
    } else if arr < target_return {
        "Reject"
    } else {
        "Indifferent"
    };
    json!({
        "calculated_arr": arr,
        "target_return": target_return,
        "decision": decision,
        "difference": arr - target_return,
    })
}

/// `/average-profit`
pub fn average_profit(total_profits: f64, years: i64) -> Result<Value, String> {
    if years == 0 {
        return Err("Years must be positive".to_string());
    }
    Ok(json!({
        "total_profits": total_profits,
        "years": years,
        "average_annual_profit": py_round2(total_profits / years as f64),
    }))
}

/// `/average-capital`
pub fn average_capital(initial_investment: f64, residual_value: f64) -> Value {
    json!({
        "initial_investment": initial_investment,
        "residual_value": residual_value,
        "average_capital": py_round2((initial_investment + residual_value) / 2.0),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cost_of_capital_adds_the_premium() {
        let out = cost_of_capital(0.12, 0.03);
        assert_eq!(out.total_cost_of_capital, 0.15);
        assert_eq!(out.as_percentage, "15.0%");
    }

    #[test]
    fn wacc_taxes_the_debt_side_only() {
        let out = wacc(400_000.0, 0.14, 300_000.0, 0.08, 25.0).unwrap();
        assert_eq!(out.after_tax_cost_debt, 0.06);
        assert_eq!(out.wacc, 0.105714);
        assert_eq!(out.wacc_percent, "10.57%");
    }

    #[test]
    fn wacc_refuses_empty_capital() {
        assert_eq!(
            wacc(0.0, 0.1, 0.0, 0.05, 0.0),
            Err("Total capital cannot be zero".to_string())
        );
    }

    #[test]
    fn discount_factors_follow_the_ladder() {
        let table = discount_factor_table(0.1, &[1, 2, 3]);
        assert_eq!(table.factors[0].discount_factor, 0.909091);
        assert_eq!(table.factors[2].discount_factor, 0.751315);
        assert_eq!(annuity_factor(0.0, 5).annuity_factor, 5.0);
    }

    #[test]
    fn discounted_payback_interpolates_the_partial_year() {
        let out = discounted_payback(100_000.0, &[40_000.0, 50_000.0, 60_000.0], 0.1);
        let payback = out.discounted_payback_period.unwrap();
        assert!(payback > 2.0 && payback < 3.0);
        assert_eq!(out.payback_achieved_in_year, Some(3));
        assert!(out.note.is_some());
    }

    #[test]
    fn discounted_payback_reports_the_shortfall_when_never_reached() {
        let out = discounted_payback(1_000_000.0, &[10_000.0, 10_000.0], 0.1);
        assert_eq!(out.discounted_payback_period, None);
        assert_eq!(out.payback_achieved_in_year, None);
        assert!(out.message.is_some());
        assert!(out.total_pv_of_cash_flows.unwrap() > 17_000.0);
    }

    #[test]
    fn arr_uses_average_profit_over_average_investment() {
        let out = accounting_rate_of_return(200_000.0, 20_000.0, 150_000.0, 3).unwrap();
        assert_eq!(out.average_annual_profit, 50_000.0);
        assert_eq!(out.average_investment, 110_000.0);
        assert_eq!(out.arr_percentage, 45.45);
        assert_eq!(out.arr_decimal, 0.4545);
    }

    #[test]
    fn arr_comparison_ranks_and_recommends() {
        let projects = vec![
            ("B".to_string(), 100_000.0, 0.0, vec![10_000.0, 10_000.0]),
            ("A".to_string(), 100_000.0, 0.0, vec![30_000.0, 30_000.0]),
            ("Empty".to_string(), 100_000.0, 0.0, vec![]),
        ];
        let out = arr_comparison(&projects);
        assert_eq!(out.project_comparison.len(), 2);
        assert_eq!(out.project_comparison[0].name, "A");
        assert_eq!(out.recommended_project, Some("A".to_string()));
    }

    #[test]
    fn arr_decision_covers_all_three_verdicts() {
        assert_eq!(arr_decision(15.0, 12.0)["decision"], "Accept");
        assert_eq!(arr_decision(9.0, 12.0)["decision"], "Reject");
        assert_eq!(arr_decision(12.0, 12.0)["decision"], "Indifferent");
    }
}
