//! profit-loss-account-service `/prepare` and `/compare`.
use serde::{Deserialize, Serialize};
use vimbai_common::py_round2;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PnLInput {
    pub company_id: String,
    pub fiscal_year_start: String,
    pub fiscal_year_end: String,
    pub revenue: f64,
    pub other_income: f64,
    pub cost_of_goods_sold: f64,
    pub operating_expenses: f64,
    pub depreciation: f64,
    pub interest_expense: f64,
    pub other_expenses: f64,
    pub tax_rate: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PnLStatement {
    pub company_id: String,
    pub period_start: String,
    pub period_end: String,
    pub revenue: f64,
    pub other_income: f64,
    pub total_revenue: f64,
    pub cost_of_goods_sold: f64,
    pub gross_profit: f64,
    pub gross_margin: f64,
    pub operating_expenses: f64,
    pub depreciation: f64,
    pub operating_income: f64,
    pub operating_margin: f64,
    pub interest_expense: f64,
    pub other_expenses: f64,
    pub pretax_income: f64,
    pub tax_expense: f64,
    pub net_income: f64,
    pub net_margin: f64,
    pub ebitda: f64,
    pub ebit: f64,
}

fn margin(num: f64, revenue: f64) -> f64 {
    if revenue == 0.0 {
        0.0
    } else {
        num / revenue * 100.0
    }
}

/// PYTHON QUIRK: gross profit and every margin use `revenue` only, while
/// `total_revenue` adds other income. EBITDA is before depreciation and
/// EBIT equals operating income, not pretax.
pub fn prepare_pnl(req: &PnLInput) -> PnLStatement {
    let total_revenue = req.revenue + req.other_income;
    let gross_profit = req.revenue - req.cost_of_goods_sold;
    let gross_margin = margin(gross_profit, req.revenue);
    let ebitda = gross_profit - req.operating_expenses;
    let ebit = ebitda - req.depreciation;
    let operating_income = ebit;
    let operating_margin = margin(operating_income, req.revenue);
    let pretax_income = operating_income - req.interest_expense - req.other_expenses;
    let tax_expense = pretax_income.max(0.0) * req.tax_rate;
    let net_income = pretax_income - tax_expense;
    let net_margin = margin(net_income, req.revenue);

    PnLStatement {
        company_id: req.company_id.clone(),
        period_start: req.fiscal_year_start.clone(),
        period_end: req.fiscal_year_end.clone(),
        revenue: req.revenue,
        other_income: req.other_income,
        total_revenue,
        cost_of_goods_sold: req.cost_of_goods_sold,
        gross_profit: py_round2(gross_profit),
        gross_margin: py_round2(gross_margin),
        operating_expenses: req.operating_expenses,
        depreciation: req.depreciation,
        operating_income: py_round2(operating_income),
        operating_margin: py_round2(operating_margin),
        interest_expense: req.interest_expense,
        other_expenses: req.other_expenses,
        pretax_income: py_round2(pretax_income),
        tax_expense: py_round2(tax_expense),
        net_income: py_round2(net_income),
        net_margin: py_round2(net_margin),
        ebitda: py_round2(ebitda),
        ebit: py_round2(ebit),
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PnLComparison {
    pub company_id: String,
    pub revenue_change_pct: f64,
    pub net_income_change_pct: f64,
    pub current_revenue: f64,
    pub previous_revenue: f64,
    pub current_net_income: f64,
    pub previous_net_income: f64,
}

/// `/compare`. The income change divides by the ABSOLUTE prior income, so
/// a swing out of a loss reads positive; a zero prior yields 0, not 100.
pub fn compare_pnl(
    req: &PnLInput,
    previous_revenue: f64,
    previous_net_income: f64,
) -> PnLComparison {
    let gross_profit = req.revenue - req.cost_of_goods_sold;
    let operating_income = gross_profit - req.operating_expenses - req.depreciation;
    let pretax = operating_income - req.interest_expense - req.other_expenses;
    let tax = pretax.max(0.0) * req.tax_rate;
    let net_income = pretax - tax;
    let revenue_change = if previous_revenue == 0.0 {
        0.0
    } else {
        (req.revenue - previous_revenue) / previous_revenue * 100.0
    };
    let income_change = if previous_net_income == 0.0 {
        0.0
    } else {
        (net_income - previous_net_income) / previous_net_income.abs() * 100.0
    };
    PnLComparison {
        company_id: req.company_id.clone(),
        revenue_change_pct: py_round2(revenue_change),
        net_income_change_pct: py_round2(income_change),
        current_revenue: req.revenue,
        previous_revenue,
        current_net_income: py_round2(net_income),
        previous_net_income,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> PnLInput {
        PnLInput {
            company_id: "c1".into(),
            fiscal_year_start: "2026-01-01".into(),
            fiscal_year_end: "2026-12-31".into(),
            revenue: 900_000.0,
            other_income: 50_000.0,
            cost_of_goods_sold: 540_000.0,
            operating_expenses: 120_000.0,
            depreciation: 30_000.0,
            interest_expense: 20_000.0,
            other_expenses: 5_000.0,
            tax_rate: 0.25,
        }
    }

    #[test]
    fn statement_ladder() {
        let r = prepare_pnl(&input());
        assert_eq!(r.total_revenue, 950_000.0);
        assert_eq!(r.gross_profit, 360_000.0);
        assert_eq!(r.gross_margin, 40.0);
        assert_eq!(r.ebitda, 240_000.0);
        assert_eq!(r.ebit, 210_000.0);
        assert_eq!(r.operating_income, 210_000.0);
        assert_eq!(r.pretax_income, 185_000.0);
        assert_eq!(r.tax_expense, 46_250.0);
        assert_eq!(r.net_income, 138_750.0);
        assert_eq!(r.net_margin, 15.42);
    }

    #[test]
    fn margins_ignore_other_income() {
        // gross margin is 360k/900k, not 360k/950k
        let r = prepare_pnl(&input());
        assert_eq!(r.gross_margin, 40.0);
        let without = prepare_pnl(&PnLInput {
            other_income: 0.0,
            ..input()
        });
        assert_eq!(r.gross_margin, without.gross_margin);
        assert_eq!(r.net_margin, without.net_margin);
    }

    #[test]
    fn zero_revenue_guards_all_margins() {
        let r = prepare_pnl(&PnLInput {
            revenue: 0.0,
            cost_of_goods_sold: 0.0,
            ..input()
        });
        assert_eq!(r.gross_margin, 0.0);
        assert_eq!(r.net_margin, 0.0);
        assert_eq!(r.total_revenue, 50_000.0);
    }

    #[test]
    fn compare_uses_absolute_prior_income() {
        let req = PnLInput {
            revenue: 1_000_000.0,
            ..input()
        };
        let c = compare_pnl(&req, 900_000.0, -50_000.0);
        assert_eq!(c.revenue_change_pct, 11.11);
        // current income is positive against a prior loss: change is > 100%
        assert!(c.net_income_change_pct > 100.0);
        let z = compare_pnl(&req, 0.0, 0.0);
        assert_eq!(z.revenue_change_pct, 0.0);
        assert_eq!(z.net_income_change_pct, 0.0);
    }
}
