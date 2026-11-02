//! interim-financial-reporting-service: IAS 34 interim statements.
//!
//! PYTHON QUIRK: the figures are generated from a per-period multiplier rather
//! than read from the books, and year to date equals the current period exactly
//! because the multiplier already carries the periods elapsed. The cash flow
//! statement's net cash flow is a fixed 200,000 rather than the sum of its
//! three activities, and the comparative balance sheet is always absent.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const PERIOD_REVENUE: f64 = 2_500_000.0;
const COMPARATIVE_REVENUE: f64 = 2_300_000.0;
const WEIGHTED_AVERAGE_SHARES: i64 = 10_000_000;
const DILUTED_SHARES: i64 = 10_500_000;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InterimPeriod {
    /// `quarterly`, `half_year` or `nine_months`.
    #[serde(default)]
    pub period_type: String,
    #[serde(default)]
    pub year: i64,
    /// One to four; it is also the multiplier behind every figure.
    #[serde(default)]
    pub period_number: i64,
    #[serde(default)]
    pub start_date: String,
    #[serde(default)]
    pub end_date: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InterimReportRequest {
    pub company_id: String,
    #[serde(default)]
    pub period: InterimPeriod,
    #[serde(default = "yes")]
    pub include_comparatives: bool,
    #[serde(default)]
    pub include_segment_breakdown: bool,
    #[serde(default = "ifrs")]
    pub accounting_standard: String,
}

fn yes() -> bool {
    true
}

fn ifrs() -> String {
    "IFRS".to_string()
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct InterimRevenue {
    pub revenue: f64,
    pub cost_of_sales: f64,
    pub gross_profit: f64,
    pub other_income: f64,
    pub distribution_costs: f64,
    pub administrative_expenses: f64,
    pub operating_profit: f64,
    pub finance_costs: f64,
    pub profit_before_tax: f64,
    pub income_tax_expense: f64,
    pub profit_after_tax: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InterimBalanceSheet {
    pub assets: BTreeMap<String, f64>,
    pub liabilities: BTreeMap<String, f64>,
    pub equity: BTreeMap<String, f64>,
    pub total_assets: f64,
    pub total_liabilities: f64,
    pub total_equity: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InterimChangesInEquity {
    pub opening_balance: f64,
    pub profit_for_period: f64,
    pub other_comprehensive_income: f64,
    pub dividends_paid: f64,
    pub share_issue: f64,
    pub closing_balance: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InterimCashFlow {
    pub operating_activities: f64,
    pub investing_activities: f64,
    pub financing_activities: f64,
    pub net_cash_flow: f64,
    pub opening_cash: f64,
    pub closing_cash: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EarningsPerShare {
    pub basic_eps: f64,
    pub diluted_eps: f64,
    pub weighted_average_shares: i64,
    pub diluted_shares: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InterimReport {
    pub company_id: String,
    pub period: InterimPeriod,
    pub currency: String,
    pub revenue: InterimRevenue,
    pub comparative_revenue: Option<InterimRevenue>,
    pub balance_sheet: InterimBalanceSheet,
    pub comparative_balance_sheet: Option<InterimBalanceSheet>,
    pub changes_in_equity: InterimChangesInEquity,
    pub cash_flow: InterimCashFlow,
    pub earnings_per_share: EarningsPerShare,
    pub year_to_date: InterimRevenue,
    pub effective_tax_rate: f64,
    pub management_commentary: BTreeMap<String, String>,
}

/// The income statement built from a base revenue and a set of cost ratios.
fn income_statement(
    revenue: f64,
    cost_ratio: f64,
    other: f64,
    dist: f64,
    admin: f64,
    finance: f64,
) -> InterimRevenue {
    let cost_of_sales = revenue * cost_ratio;
    let gross_profit = revenue - cost_of_sales;
    let other_income = revenue * other;
    let distribution_costs = revenue * dist;
    let administrative_expenses = revenue * admin;
    let operating_profit =
        gross_profit + other_income - distribution_costs - administrative_expenses;
    let finance_costs = operating_profit * finance;
    let profit_before_tax = operating_profit - finance_costs;
    let income_tax_expense = profit_before_tax * 0.25;
    InterimRevenue {
        revenue,
        cost_of_sales,
        gross_profit,
        other_income,
        distribution_costs,
        administrative_expenses,
        operating_profit,
        finance_costs,
        profit_before_tax,
        income_tax_expense,
        profit_after_tax: profit_before_tax - income_tax_expense,
    }
}

/// Depreciation on 35% of assets at ten percent.
pub fn depreciation(total_assets: f64) -> f64 {
    total_assets * 0.35 * 0.1
}

fn balance_sheet(total_assets: f64) -> InterimBalanceSheet {
    let total_liabilities = total_assets * 0.6;
    let total_equity = total_assets - total_liabilities;
    let mut assets = BTreeMap::new();
    assets.insert("property_plant_equipment".to_string(), total_assets * 0.35);
    assets.insert("intangible_assets".to_string(), total_assets * 0.15);
    assets.insert("inventory".to_string(), total_assets * 0.12);
    assets.insert("trade_receivables".to_string(), total_assets * 0.15);
    assets.insert("cash_and_equivalents".to_string(), total_assets * 0.10);
    assets.insert("other_current_assets".to_string(), total_assets * 0.13);

    let mut liabilities = BTreeMap::new();
    liabilities.insert("long_term_borrowings".to_string(), total_liabilities * 0.30);
    liabilities.insert("deferred_tax".to_string(), total_liabilities * 0.10);
    liabilities.insert("trade_payables".to_string(), total_liabilities * 0.20);
    liabilities.insert(
        "short_term_borrowings".to_string(),
        total_liabilities * 0.15,
    );
    liabilities.insert("current_tax".to_string(), total_liabilities * 0.08);
    liabilities.insert("accruals".to_string(), total_liabilities * 0.17);

    let mut equity = BTreeMap::new();
    equity.insert("share_capital".to_string(), total_equity * 0.20);
    equity.insert("share_premium".to_string(), total_equity * 0.15);
    equity.insert("retained_earnings".to_string(), total_equity * 0.50);
    equity.insert("other_reserves".to_string(), total_equity * 0.15);

    InterimBalanceSheet {
        assets,
        liabilities,
        equity,
        total_assets,
        total_liabilities,
        total_equity,
    }
}

/// `/prepare`. The period count is the caller's to validate (the service takes
/// one to four via the model).
pub fn prepare(req: &InterimReportRequest) -> InterimReport {
    let multiplier = req.period.period_number as f64;
    let revenue = PERIOD_REVENUE * multiplier;
    let current = income_statement(revenue, 0.55, 0.03, 0.08, 0.12, 0.05);

    let comparative = if req.include_comparatives {
        Some(income_statement(
            COMPARATIVE_REVENUE * multiplier,
            0.58,
            0.025,
            0.07,
            0.11,
            0.06,
        ))
    } else {
        None
    };

    let sheet = balance_sheet(revenue * 8.0);
    let profit_after_tax = current.profit_after_tax;

    let changes_in_equity = InterimChangesInEquity {
        opening_balance: sheet.total_equity * 0.85,
        profit_for_period: profit_after_tax,
        other_comprehensive_income: 50_000.0,
        dividends_paid: -150_000.0,
        share_issue: 0.0,
        closing_balance: sheet.total_equity,
    };

    let cash_flow = InterimCashFlow {
        operating_activities: profit_after_tax + depreciation(sheet.total_assets),
        investing_activities: -300_000.0 * multiplier,
        financing_activities: -100_000.0 * multiplier,
        // fixed, not the sum of the three activities
        net_cash_flow: 200_000.0,
        opening_cash: sheet.total_assets * 0.08,
        closing_cash: sheet.total_assets * 0.10,
    };

    let mut commentary = BTreeMap::new();
    commentary.insert(
        "revenue_change".to_string(),
        format!("+{:.1}%", (multiplier - 1.0) * 100.0),
    );
    commentary.insert(
        "gross_margin".to_string(),
        format!("{:.1}%", current.gross_profit / current.revenue * 100.0),
    );
    commentary.insert(
        "operating_margin".to_string(),
        format!("{:.1}%", current.operating_profit / current.revenue * 100.0),
    );

    InterimReport {
        company_id: req.company_id.clone(),
        period: req.period.clone(),
        currency: "USD".to_string(),
        revenue: current.clone(),
        comparative_revenue: comparative,
        balance_sheet: sheet,
        comparative_balance_sheet: None,
        changes_in_equity,
        cash_flow,
        earnings_per_share: EarningsPerShare {
            basic_eps: profit_after_tax / WEIGHTED_AVERAGE_SHARES as f64,
            diluted_eps: profit_after_tax / DILUTED_SHARES as f64,
            weighted_average_shares: WEIGHTED_AVERAGE_SHARES,
            diluted_shares: DILUTED_SHARES,
        },
        // year to date equals the period, because the multiplier carries it
        year_to_date: current.clone(),
        effective_tax_rate: if current.profit_before_tax > 0.0 {
            current.income_tax_expense / current.profit_before_tax
        } else {
            0.0
        },
        management_commentary: commentary,
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct YearToDate {
    pub year: i64,
    pub periods_included: i64,
    pub ytd_revenue: f64,
    pub ytd_profit: f64,
    pub ytd_cash_flow: f64,
}

pub fn year_to_date(period: &InterimPeriod) -> YearToDate {
    let ytd_revenue = PERIOD_REVENUE * period.period_number as f64;
    let ytd_profit = ytd_revenue * 0.12;
    YearToDate {
        year: period.year,
        periods_included: period.period_number,
        ytd_revenue,
        ytd_profit,
        ytd_cash_flow: ytd_profit * 0.8,
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EffectiveTaxRate {
    pub profit_before_tax: f64,
    pub income_tax_expense: f64,
    pub effective_tax_rate: f64,
    pub standard_tax_rate: f64,
    pub difference: f64,
}

pub fn effective_tax_rate(period: &InterimPeriod) -> EffectiveTaxRate {
    let profit_before_tax = PERIOD_REVENUE * period.period_number as f64 * 0.12;
    let income_tax_expense = profit_before_tax * 0.25;
    EffectiveTaxRate {
        profit_before_tax,
        income_tax_expense,
        effective_tax_rate: 0.25,
        standard_tax_rate: 0.25,
        difference: 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(period_number: i64) -> InterimReportRequest {
        InterimReportRequest {
            company_id: "c1".to_string(),
            period: InterimPeriod {
                period_type: "quarterly".to_string(),
                year: 2026,
                period_number,
                start_date: "2026-01-01".to_string(),
                end_date: "2026-03-31".to_string(),
            },
            include_comparatives: true,
            include_segment_breakdown: false,
            accounting_standard: "IFRS".to_string(),
        }
    }

    #[test]
    fn the_multiplier_scales_every_figure() {
        let one = prepare(&request(1));
        let two = prepare(&request(2));
        assert_eq!(one.revenue.revenue, 2_500_000.0);
        assert_eq!(two.revenue.revenue, 5_000_000.0);
        assert!((two.revenue.cost_of_sales / one.revenue.cost_of_sales - 2.0).abs() < 1e-12);
        assert_eq!(two.balance_sheet.total_assets, 40_000_000.0);
        assert_eq!(two.balance_sheet.total_liabilities, 24_000_000.0);
        assert_eq!(two.balance_sheet.total_equity, 16_000_000.0);
    }

    #[test]
    fn year_to_date_matches_the_period() {
        let report = prepare(&request(3));
        assert_eq!(report.year_to_date, report.revenue);
    }

    #[test]
    fn the_comparative_uses_the_prior_year_margins() {
        let report = prepare(&request(1));
        let comp = report.comparative_revenue.unwrap();
        assert_eq!(comp.revenue, 2_300_000.0);
        // 58% cost ratio against the current 55%
        assert!((comp.cost_of_sales - 1_334_000.0).abs() < 1e-9);
        assert!(report.comparative_balance_sheet.is_none());
        assert!(prepare(&InterimReportRequest {
            include_comparatives: false,
            ..request(1)
        })
        .comparative_revenue
        .is_none());
    }

    #[test]
    fn net_cash_flow_is_not_the_sum_of_its_activities() {
        let report = prepare(&request(1));
        let flow = &report.cash_flow;
        assert_eq!(flow.net_cash_flow, 200_000.0);
        let sum = flow.operating_activities + flow.investing_activities + flow.financing_activities;
        assert!((sum - flow.net_cash_flow).abs() > 1.0);
        assert_eq!(flow.investing_activities, -300_000.0);
        assert_eq!(
            report.cash_flow.opening_cash,
            report.balance_sheet.total_assets * 0.08
        );
    }

    #[test]
    fn the_earnings_per_share_uses_two_share_counts() {
        let report = prepare(&request(2));
        let pat = report.revenue.profit_after_tax;
        assert!((report.earnings_per_share.basic_eps - pat / 10_000_000.0).abs() < 1e-15);
        assert!((report.earnings_per_share.diluted_eps - pat / 10_500_000.0).abs() < 1e-15);
        assert_eq!(report.earnings_per_share.diluted_shares, 10_500_000);
    }

    #[test]
    fn the_commentary_counts_periods_not_years() {
        let first = prepare(&request(1));
        assert_eq!(first.management_commentary["revenue_change"], "+0.0%");
        assert_eq!(first.management_commentary["gross_margin"], "45.0%");
        let third = prepare(&request(3));
        assert_eq!(third.management_commentary["revenue_change"], "+200.0%");
        // the margins do not move with the multiplier
        assert_eq!(third.management_commentary["gross_margin"], "45.0%");
    }

    #[test]
    fn the_effective_rate_is_the_statutory_rate() {
        let period = request(2).period;
        let rate = effective_tax_rate(&period);
        assert_eq!(rate.profit_before_tax, 600_000.0);
        assert_eq!(rate.income_tax_expense, 150_000.0);
        assert_eq!(rate.effective_tax_rate, 0.25);
        assert_eq!(rate.difference, 0.0);
        assert_eq!(prepare(&request(2)).effective_tax_rate, 0.25);
    }

    #[test]
    fn year_to_date_takes_the_period_count() {
        let ytd = year_to_date(&request(3).period);
        assert_eq!(ytd.periods_included, 3);
        assert_eq!(ytd.ytd_revenue, 7_500_000.0);
        assert_eq!(ytd.ytd_profit, 900_000.0);
        assert_eq!(ytd.ytd_cash_flow, 720_000.0);
    }

    #[test]
    fn an_empty_period_reports_nothing_rather_than_dividing() {
        let report = prepare(&InterimReportRequest::default());
        assert_eq!(report.revenue.revenue, 0.0);
        assert_eq!(report.effective_tax_rate, 0.0);
        assert_eq!(report.management_commentary["revenue_change"], "+-100.0%");
    }
}
