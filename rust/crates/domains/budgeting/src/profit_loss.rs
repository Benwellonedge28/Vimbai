#![forbid(unsafe_code)]
//! Profit and loss, ported from profit-loss-account-service `/prepare` and `/compare`.
//!
//! Kept as shipped:
//!
//! * Gross profit and every margin divide by `revenue` alone, while
//!   `total_revenue` adds other income.
//! * EBITDA is taken before depreciation and EBIT equals operating income, not
//!   pretax income.
//! * `/compare` divides the income change by the ABSOLUTE prior income, so a
//!   swing out of a loss reads as a rise of more than 100%, and a zero prior
//!   yields 0 rather than 100.
//!
//! Every amount is Money end to end, so a revenue past double precision keeps
//! its digits; the tax rate stays a ratio. One endpoint detail: the Python took
//! `company_id` as a separate query parameter on `/compare` and returned that,
//! while the body carried its own. The merged request has one company id and it
//! is the one returned.

use crate::figures::{money, r2};
use serde::{Deserialize, Serialize};
use vimbai_common::Money;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PnLInput {
    pub company_id: String,
    #[serde(default)]
    pub fiscal_year_start: String,
    #[serde(default)]
    pub fiscal_year_end: String,
    pub revenue: Money,
    #[serde(default = "zero_amount")]
    pub other_income: Money,
    #[serde(default = "zero_amount")]
    pub cost_of_goods_sold: Money,
    #[serde(default = "zero_amount")]
    pub operating_expenses: Money,
    #[serde(default = "zero_amount")]
    pub depreciation: Money,
    #[serde(default = "zero_amount")]
    pub interest_expense: Money,
    #[serde(default = "zero_amount")]
    pub other_expenses: Money,
    #[serde(default = "default_tax_rate")]
    pub tax_rate: f64,
}

fn default_tax_rate() -> f64 {
    0.25
}

/// `#[serde(default)]` on an amount needs a `Money: Default`, which the shared type
/// does not have; a named zero keeps the published shapes lenient the same way.
fn zero_amount() -> Money {
    Money::zero()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PnLStatement {
    pub company_id: String,
    pub period_start: String,
    pub period_end: String,
    pub revenue: Money,
    pub other_income: Money,
    pub total_revenue: Money,
    pub cost_of_goods_sold: Money,
    pub gross_profit: Money,
    pub gross_margin: f64,
    pub operating_expenses: Money,
    pub depreciation: Money,
    pub operating_income: Money,
    pub operating_margin: f64,
    pub interest_expense: Money,
    pub other_expenses: Money,
    pub pretax_income: Money,
    pub tax_expense: Money,
    pub net_income: Money,
    pub net_margin: f64,
    pub ebitda: Money,
    pub ebit: Money,
}

fn margin(amount: &Money, revenue: &Money) -> f64 {
    if revenue.is_zero() {
        0.0
    } else {
        amount.over(revenue).to_f64_lossy() * 100.0
    }
}

/// PYTHON QUIRK: gross profit and every margin use `revenue` only, while
/// `total_revenue` adds other income. EBITDA is before depreciation and EBIT
/// equals operating income, not pretax.
pub fn prepare_pnl(req: &PnLInput) -> PnLStatement {
    let total_revenue = req.revenue.plus(&req.other_income);
    let gross_profit = req.revenue.minus(&req.cost_of_goods_sold);
    let gross_margin = margin(&gross_profit, &req.revenue);
    let ebitda = gross_profit.minus(&req.operating_expenses);
    let depreciated = ebitda.minus(&req.depreciation);
    let operating_income = depreciated.clone();
    let operating_margin = margin(&operating_income, &req.revenue);
    let pretax_income = operating_income
        .minus(&req.interest_expense)
        .minus(&req.other_expenses);
    // A loss is not taxed: max(pretax, 0) * rate, as the service had it.
    let taxable = if pretax_income > Money::zero() {
        pretax_income.clone()
    } else {
        Money::zero()
    };
    let tax_expense = taxable.times(&money(req.tax_rate)).round(2);
    let net_income = pretax_income.minus(&tax_expense);
    let net_margin = margin(&net_income, &req.revenue);

    PnLStatement {
        company_id: req.company_id.clone(),
        period_start: req.fiscal_year_start.clone(),
        period_end: req.fiscal_year_end.clone(),
        revenue: req.revenue.clone(),
        other_income: req.other_income.clone(),
        total_revenue,
        cost_of_goods_sold: req.cost_of_goods_sold.clone(),
        gross_profit: gross_profit.round(2),
        gross_margin: r2(gross_margin),
        operating_expenses: req.operating_expenses.clone(),
        depreciation: req.depreciation.clone(),
        operating_income: operating_income.round(2),
        operating_margin: r2(operating_margin),
        interest_expense: req.interest_expense.clone(),
        other_expenses: req.other_expenses.clone(),
        pretax_income: pretax_income.round(2),
        tax_expense,
        net_income: net_income.round(2),
        net_margin: r2(net_margin),
        ebitda: ebitda.round(2),
        ebit: depreciated.round(2),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PnLComparison {
    pub company_id: String,
    pub revenue_change_pct: f64,
    pub net_income_change_pct: f64,
    pub current_revenue: Money,
    pub previous_revenue: Money,
    pub current_net_income: Money,
    pub previous_net_income: Money,
}

/// `/compare`. The income change divides by the ABSOLUTE prior income, so a
/// swing out of a loss reads positive; a zero prior yields 0, not 100.
pub fn compare_pnl(
    req: &PnLInput,
    previous_revenue: &Money,
    previous_net_income: &Money,
) -> PnLComparison {
    let gross_profit = req.revenue.minus(&req.cost_of_goods_sold);
    let operating_income = gross_profit
        .minus(&req.operating_expenses)
        .minus(&req.depreciation);
    let pretax = operating_income
        .minus(&req.interest_expense)
        .minus(&req.other_expenses);
    let taxable = if pretax > Money::zero() {
        pretax.clone()
    } else {
        Money::zero()
    };
    let tax = taxable.times(&money(req.tax_rate)).round(2);
    let net_income = pretax.minus(&tax);
    let revenue_change = if previous_revenue.is_zero() {
        0.0
    } else {
        r2(req
            .revenue
            .minus(previous_revenue)
            .over(previous_revenue)
            .to_f64_lossy()
            * 100.0)
    };
    let income_change = if previous_net_income.is_zero() {
        0.0
    } else {
        let prior = previous_net_income.abs();
        r2(net_income
            .minus(previous_net_income)
            .over(&prior)
            .to_f64_lossy()
            * 100.0)
    };
    PnLComparison {
        company_id: req.company_id.clone(),
        revenue_change_pct: revenue_change,
        net_income_change_pct: income_change,
        current_revenue: req.revenue.clone(),
        previous_revenue: previous_revenue.clone(),
        current_net_income: net_income.round(2),
        previous_net_income: previous_net_income.clone(),
    }
}

/// `profit-loss-account-service /prepare`.
pub fn profit_loss_prepare_from_json(
    payload: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let request: PnLInput = serde_json::from_value(payload.clone())
        .map_err(|error| format!("invalid request: {error}"))?;
    serde_json::to_value(prepare_pnl(&request)).map_err(|error| error.to_string())
}

/// `profit-loss-account-service /compare`.
pub fn profit_loss_compare_from_json(
    payload: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    #[derive(Deserialize)]
    struct Request {
        #[serde(flatten)]
        current: PnLInput,
        previous_revenue: Money,
        previous_net_income: Money,
    }
    let request: Request = serde_json::from_value(payload.clone())
        .map_err(|error| format!("invalid request: {error}"))?;
    serde_json::to_value(compare_pnl(
        &request.current,
        &request.previous_revenue,
        &request.previous_net_income,
    ))
    .map_err(|error| error.to_string())
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

    fn input() -> PnLInput {
        PnLInput {
            company_id: "c1".into(),
            fiscal_year_start: "2026-01-01".into(),
            fiscal_year_end: "2026-12-31".into(),
            revenue: m("900000"),
            other_income: m("50000"),
            cost_of_goods_sold: m("540000"),
            operating_expenses: m("120000"),
            depreciation: m("30000"),
            interest_expense: m("20000"),
            other_expenses: m("5000"),
            tax_rate: 0.25,
        }
    }

    #[test]
    fn statement_ladder() {
        let r = prepare_pnl(&input());
        assert_eq!(fixed(&r.total_revenue), "950000.00");
        assert_eq!(fixed(&r.gross_profit), "360000.00");
        assert_eq!(r.gross_margin, 40.0);
        assert_eq!(fixed(&r.ebitda), "240000.00");
        assert_eq!(fixed(&r.ebit), "210000.00");
        assert_eq!(fixed(&r.operating_income), "210000.00");
        assert_eq!(fixed(&r.pretax_income), "185000.00");
        assert_eq!(fixed(&r.tax_expense), "46250.00");
        assert_eq!(fixed(&r.net_income), "138750.00");
        assert_eq!(r.net_margin, 15.42);
    }

    #[test]
    fn margins_ignore_other_income() {
        // gross margin is 360k/900k, not 360k/950k
        let r = prepare_pnl(&input());
        assert_eq!(r.gross_margin, 40.0);
        let mut without = input();
        without.other_income = Money::zero();
        let without = prepare_pnl(&without);
        assert_eq!(r.gross_margin, without.gross_margin);
        assert_eq!(r.net_margin, without.net_margin);
    }

    #[test]
    fn a_loss_is_not_taxed() {
        let mut losing = input();
        losing.revenue = m("100000");
        let r = prepare_pnl(&losing);
        assert_eq!(fixed(&r.tax_expense), "0.00");
        assert_eq!(fixed(&r.net_income), fixed(&r.pretax_income));
    }

    #[test]
    fn zero_revenue_guards_all_margins() {
        let mut zero = input();
        zero.revenue = Money::zero();
        zero.cost_of_goods_sold = Money::zero();
        let r = prepare_pnl(&zero);
        assert_eq!(r.gross_margin, 0.0);
        assert_eq!(r.net_margin, 0.0);
        assert_eq!(fixed(&r.total_revenue), "50000.00");
    }

    #[test]
    fn compare_uses_absolute_prior_income() {
        let mut req = input();
        req.revenue = m("1000000");
        let c = compare_pnl(&req, &m("900000"), &m("-50000"));
        assert_eq!(c.revenue_change_pct, 11.11);
        // current income is positive against a prior loss: change is > 100%
        assert!(c.net_income_change_pct > 100.0);
        let z = compare_pnl(&req, &m("0"), &m("0"));
        assert_eq!(z.revenue_change_pct, 0.0);
        assert_eq!(z.net_income_change_pct, 0.0);
    }

    #[test]
    fn a_revenue_past_double_precision_keeps_every_digit() {
        let mut huge = input();
        huge.revenue = m("123456789012345678901.55");
        huge.cost_of_goods_sold = m("23456789012345678901.55");
        huge.operating_expenses = m("10000000000000000000.00");
        huge.depreciation = m("0");
        huge.interest_expense = m("0");
        huge.other_expenses = m("0");
        huge.other_income = m("0");
        let r = prepare_pnl(&huge);
        assert_eq!(fixed(&r.gross_profit), "100000000000000000000.00");
        assert_eq!(fixed(&r.ebitda), "90000000000000000000.00");
        assert_eq!(fixed(&r.tax_expense), "22500000000000000000.00");
        assert_eq!(fixed(&r.net_income), "67500000000000000000.00");
    }

    #[test]
    fn the_doors_read_the_published_shape() {
        let body = serde_json::json!({
            "company_id": "c1", "fiscal_year_start": "2026-01-01",
            "fiscal_year_end": "2026-12-31", "revenue": "900000",
            "other_income": "50000", "cost_of_goods_sold": "540000",
            "operating_expenses": "120000", "depreciation": "30000",
            "interest_expense": "20000", "other_expenses": "5000", "tax_rate": 0.25
        });
        let prepared = profit_loss_prepare_from_json(&body).expect("prepare");
        assert_eq!(prepared["net_income"], "138750.00");

        // the tax rate defaults to a quarter when the caller omits it
        let lenient = profit_loss_prepare_from_json(&serde_json::json!({
            "company_id": "c1", "revenue": "1000", "cost_of_goods_sold": "0"
        }))
        .expect("prepare");
        assert_eq!(lenient["tax_expense"], "250.00");

        let mut compare_body = body.clone();
        compare_body["previous_revenue"] = serde_json::json!("800000");
        compare_body["previous_net_income"] = serde_json::json!("100000");
        let compared = profit_loss_compare_from_json(&compare_body).expect("compare");
        assert_eq!(compared["revenue_change_pct"], 12.5);
        assert_eq!(compared["current_net_income"], "138750.00");
        assert_eq!(compared["company_id"], "c1");

        let bad = profit_loss_prepare_from_json(&serde_json::json!({
            "company_id": "c1", "revenue": "not money"
        }))
        .expect_err("refused");
        assert!(bad.contains("invalid request"), "{bad}");
    }
}
