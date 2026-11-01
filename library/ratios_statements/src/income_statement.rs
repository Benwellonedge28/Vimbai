//! income-statement-service `/prepare`: full statement build-up from
//! revenue through EBITDA, EBIT, tax and net income with margins.
use serde::{Deserialize, Serialize};
use vimbai_common::py_round2;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IncomeStatementInput {
    pub company_id: String,
    pub fiscal_year: String,
    pub revenue: f64,
    pub other_income: f64,
    pub cost_of_goods_sold: f64,
    pub operating_expenses: f64,
    pub depreciation: f64,
    pub amortization: f64,
    pub interest_expense: f64,
    pub other_expenses: f64,
    pub tax_rate: f64,
    #[serde(default)]
    pub shares_outstanding: Option<f64>,
    #[serde(default)]
    pub diluted_shares: Option<f64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IncomeStatementOutput {
    pub company_id: String,
    pub fiscal_year: String,
    pub revenue: f64,
    pub other_income: f64,
    pub total_revenue: f64,
    pub cogs: f64,
    pub gross_profit: f64,
    pub gross_margin: f64,
    pub operating_expenses: f64,
    pub depreciation_amortization: f64,
    pub operating_income: f64,
    pub operating_margin: f64,
    pub interest_expense: f64,
    pub other_expenses: f64,
    pub pretax_income: f64,
    pub tax_expense: f64,
    pub net_income: f64,
    pub net_margin: f64,
    pub eps: Option<f64>,
    pub diluted_eps: Option<f64>,
    pub ebitda: f64,
    pub ebit: f64,
}

fn div(num: f64, den: f64) -> f64 {
    if den == 0.0 {
        0.0
    } else {
        num / den
    }
}

pub fn prepare_income_statement(input: &IncomeStatementInput) -> IncomeStatementOutput {
    let total_revenue = input.revenue + input.other_income;
    let gross_profit = input.revenue - input.cost_of_goods_sold;
    let gross_margin = div(gross_profit * 100.0, input.revenue);
    let da = input.depreciation + input.amortization;
    let operating_income = gross_profit - input.operating_expenses - da;
    let operating_margin = div(operating_income * 100.0, input.revenue);
    let ebitda = gross_profit - input.operating_expenses;
    let pretax = operating_income - input.interest_expense - input.other_expenses;
    // PYTHON QUIRK: tax base floors at zero, so a loss attracts no credit.
    let tax = pretax.max(0.0) * input.tax_rate;
    let net_income = pretax - tax;
    let net_margin = div(net_income * 100.0, input.revenue);
    let eps = input
        .shares_outstanding
        .filter(|s| *s != 0.0)
        .map(|s| py_round2(net_income / s));
    let diluted_eps = input
        .diluted_shares
        .filter(|s| *s != 0.0)
        .map(|s| py_round2(net_income / s));

    IncomeStatementOutput {
        company_id: input.company_id.clone(),
        fiscal_year: input.fiscal_year.clone(),
        revenue: py_round2(input.revenue),
        other_income: py_round2(input.other_income),
        total_revenue: py_round2(total_revenue),
        cogs: py_round2(input.cost_of_goods_sold),
        gross_profit: py_round2(gross_profit),
        gross_margin: py_round2(gross_margin),
        operating_expenses: py_round2(input.operating_expenses),
        depreciation_amortization: py_round2(da),
        operating_income: py_round2(operating_income),
        operating_margin: py_round2(operating_margin),
        interest_expense: py_round2(input.interest_expense),
        other_expenses: py_round2(input.other_expenses),
        pretax_income: py_round2(pretax),
        tax_expense: py_round2(tax),
        net_income: py_round2(net_income),
        net_margin: py_round2(net_margin),
        eps,
        diluted_eps,
        ebitda: py_round2(ebitda),
        ebit: py_round2(operating_income),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> IncomeStatementInput {
        IncomeStatementInput {
            company_id: "c1".into(),
            fiscal_year: "2026".into(),
            revenue: 1_000_000.0,
            other_income: 20_000.0,
            cost_of_goods_sold: 600_000.0,
            operating_expenses: 150_000.0,
            depreciation: 40_000.0,
            amortization: 10_000.0,
            interest_expense: 30_000.0,
            other_expenses: 5_000.0,
            tax_rate: 0.25,
            shares_outstanding: Some(100_000.0),
            diluted_shares: None,
        }
    }

    #[test]
    fn build_up_from_revenue_to_net_income() {
        let r = prepare_income_statement(&input());
        assert_eq!(r.total_revenue, 1_020_000.0);
        assert_eq!(r.gross_profit, 400_000.0);
        assert_eq!(r.gross_margin, 40.0);
        assert_eq!(r.depreciation_amortization, 50_000.0);
        assert_eq!(r.ebitda, 250_000.0);
        assert_eq!(r.operating_income, 200_000.0);
        assert_eq!(r.ebit, 200_000.0);
        assert_eq!(r.pretax_income, 165_000.0);
        assert_eq!(r.tax_expense, 41_250.0);
        assert_eq!(r.net_income, 123_750.0);
        assert_eq!(r.net_margin, 12.38);
        assert_eq!(r.eps, Some(1.24));
        assert_eq!(r.diluted_eps, None);
    }

    #[test]
    fn loss_gets_no_tax_credit() {
        let r = prepare_income_statement(&IncomeStatementInput {
            revenue: 100_000.0,
            cost_of_goods_sold: 80_000.0,
            operating_expenses: 40_000.0,
            ..input()
        });
        assert!(r.pretax_income < 0.0);
        assert_eq!(r.tax_expense, 0.0);
        assert_eq!(r.net_income, r.pretax_income);
    }
}
