//! The working capital family: analysis, working-capital finance with
//! invoice factoring, and cycle optimisation.
use serde::{Deserialize, Serialize};
use vimbai_common::py_round2;

fn ratio(num: f64, den: f64) -> f64 {
    if den == 0.0 {
        0.0
    } else {
        num / den
    }
}

fn round4(v: f64) -> f64 {
    (v * 10_000.0).round() / 10_000.0
}

fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WcAnalysisInput {
    pub company_id: String,
    pub current_assets: f64,
    pub current_liabilities: f64,
    pub inventory: f64,
    pub cash: f64,
    pub accounts_receivable: f64,
    pub accounts_payable: f64,
    pub revenue: f64,
    pub cost_of_goods_sold: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WcMetrics {
    pub current_ratio: f64,
    pub quick_ratio: f64,
    pub cash_ratio: f64,
    pub working_capital: f64,
    pub nwc: f64,
    pub operating_cycle_days: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WcAnalysis {
    pub company_id: String,
    pub metrics: WcMetrics,
    pub cash_conversion_cycle: f64,
    pub working_capital_turnover: f64,
    pub receivables_turnover: f64,
    pub payables_turnover: f64,
    pub inventory_turnover: f64,
    pub recommendations: Vec<String>,
}

/// `working-capital-service` `/analyze`. NWC is working capital less cash;
/// every denominator is guarded, so a missing inventory or receivable
/// yields 0 rather than an error.
pub fn analyze_working_capital(req: &WcAnalysisInput) -> WcAnalysis {
    let working_capital = req.current_assets - req.current_liabilities;
    let nwc = working_capital - req.cash;
    let current_ratio = ratio(req.current_assets, req.current_liabilities);
    let quick_ratio = ratio(req.current_assets - req.inventory, req.current_liabilities);
    let cash_ratio = ratio(req.cash, req.current_liabilities);
    let inv_turnover = ratio(req.cost_of_goods_sold, req.inventory);
    let rec_turnover = ratio(req.revenue, req.accounts_receivable);
    let pay_turnover = ratio(req.cost_of_goods_sold, req.accounts_payable);
    let inv_days = ratio(365.0, inv_turnover);
    let rec_days = ratio(365.0, rec_turnover);
    let pay_days = ratio(365.0, pay_turnover);
    let operating_cycle = inv_days + rec_days;
    let ccc = operating_cycle - pay_days;
    let wc_turnover = ratio(req.revenue, working_capital);

    let mut recommendations = Vec::new();
    if current_ratio < 1.5 {
        recommendations.push("Current ratio below 1.5 - consider improving liquidity".to_string());
    }
    if ccc > 90.0 {
        recommendations.push(
            "Cash conversion cycle is high - focus on reducing inventory and receivables days"
                .to_string(),
        );
    }
    if quick_ratio < 1.0 {
        recommendations
            .push("Quick ratio below 1.0 - improve immediate liquidity position".to_string());
    }

    WcAnalysis {
        company_id: req.company_id.clone(),
        metrics: WcMetrics {
            current_ratio: round4(current_ratio),
            quick_ratio: round4(quick_ratio),
            cash_ratio: round4(cash_ratio),
            working_capital: py_round2(working_capital),
            nwc: py_round2(nwc),
            operating_cycle_days: py_round2(operating_cycle),
        },
        cash_conversion_cycle: py_round2(ccc),
        working_capital_turnover: round4(wc_turnover),
        receivables_turnover: round4(rec_turnover),
        payables_turnover: round4(pay_turnover),
        inventory_turnover: round4(inv_turnover),
        recommendations,
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WcFinanceInput {
    pub company_id: String,
    pub period: String,
    pub current_assets: f64,
    pub current_liabilities: f64,
    pub inventory: f64,
    pub accounts_receivable: f64,
    pub accounts_payable: f64,
    pub annual_revenue: f64,
    pub cogs: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WcFinanceResult {
    pub company_id: String,
    pub period: String,
    pub working_capital: f64,
    pub current_ratio: f64,
    pub quick_ratio: f64,
    pub cash_conversion_cycle: f64,
    pub dso: f64,
    pub dio: f64,
    pub dpo: f64,
    pub financing_need: f64,
    pub recommendation: String,
}

/// `working-capital-finance-service` `/analyze`. Financing need only
/// appears when liabilities exceed assets, and the advice falls into one
/// of three fixed bands.
pub fn analyze_wc_finance(req: &WcFinanceInput) -> WcFinanceResult {
    let wc = req.current_assets - req.current_liabilities;
    let current_ratio = ratio(req.current_assets, req.current_liabilities);
    let quick_ratio = ratio(req.current_assets - req.inventory, req.current_liabilities);
    let dso = ratio(req.accounts_receivable * 365.0, req.annual_revenue);
    let dio = ratio(req.inventory * 365.0, req.cogs);
    let dpo = ratio(req.accounts_payable * 365.0, req.cogs);
    let ccc = dso + dio - dpo;
    let financing_need = (req.current_liabilities - req.current_assets).max(0.0);

    let recommendation = if current_ratio >= 2.0 && quick_ratio >= 1.0 {
        "Strong working capital position - no immediate financing needed"
    } else if current_ratio >= 1.0 {
        "Adequate but monitor - consider optimizing inventory and receivables"
    } else {
        "Working capital deficit - immediate short-term financing recommended"
    };

    WcFinanceResult {
        company_id: req.company_id.clone(),
        period: req.period.clone(),
        working_capital: py_round2(wc),
        current_ratio: py_round2(current_ratio),
        quick_ratio: py_round2(quick_ratio),
        cash_conversion_cycle: round1(ccc),
        dso: round1(dso),
        dio: round1(dio),
        dpo: round1(dpo),
        financing_need: py_round2(financing_need),
        recommendation: recommendation.to_string(),
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FactoringInput {
    pub invoice_amount: f64,
    pub advance_rate: f64,
    pub factor_fee_rate: f64,
    pub discount_rate: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FactoringResult {
    pub invoice_amount: f64,
    pub advance_amount: f64,
    pub fee: f64,
    pub discount: f64,
    pub net_to_company: f64,
    pub total_cost: f64,
    pub effective_annual_rate: f64,
}

/// `/factoring`. The effective annual rate assumes a fixed 60-day cycle
/// and annualises the cost against the net advance; it is 0 unless the
/// advance exceeds the total cost.
pub fn calculate_factoring(req: &FactoringInput) -> FactoringResult {
    let advance = req.invoice_amount * req.advance_rate;
    let fee = req.invoice_amount * req.factor_fee_rate;
    let discount = req.invoice_amount * req.discount_rate;
    let net = advance - fee;
    let total_cost = fee + discount;
    let ear = if advance > total_cost {
        total_cost / (advance - total_cost) * 365.0 / 60.0 * 100.0
    } else {
        0.0
    };
    FactoringResult {
        invoice_amount: py_round2(req.invoice_amount),
        advance_amount: py_round2(advance),
        fee: py_round2(fee),
        discount: py_round2(discount),
        net_to_company: py_round2(net),
        total_cost: py_round2(total_cost),
        effective_annual_rate: py_round2(ear),
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WcOptimizeInput {
    pub company_id: String,
    pub accounts_receivable: f64,
    pub accounts_payable: f64,
    pub inventory: f64,
    pub revenue: f64,
    pub purchases: f64,
    pub cogs: f64,
    pub days_analysis: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WcOptimization {
    pub company_id: String,
    pub dso: f64,
    pub dpo: f64,
    pub dio: f64,
    pub cash_conversion_cycle: f64,
    pub working_capital: f64,
    pub current_ratio: f64,
    pub quick_ratio: f64,
    pub net_working_capital: f64,
    pub recommendations: Vec<(String, String)>,
}

/// Which input the optimisation divided by zero.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WcError {
    /// `revenue`
    Revenue,
    /// `purchases`
    Purchases,
    /// `cogs`
    Cogs,
    /// `accounts_payable`
    AccountsPayable,
}

impl WcError {
    pub fn field(&self) -> &'static str {
        match self {
            WcError::Revenue => "revenue",
            WcError::Purchases => "purchases",
            WcError::Cogs => "cogs",
            WcError::AccountsPayable => "accounts_payable",
        }
    }
}

impl std::fmt::Display for WcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} must be non-zero", self.field())
    }
}

impl std::error::Error for WcError {}

/// `working-capital-optimization-service` `/optimize`.
///
/// PYTHON QUIRK: unlike the other two services this one has NO zero
/// guards, so a zero revenue, purchases, cogs or payables raises a
/// ZeroDivisionError and returns a 500. We surface that as a typed error
/// instead of returning inf/NaN.
pub fn optimize_working_capital(req: &WcOptimizeInput) -> Result<WcOptimization, WcError> {
    if req.revenue == 0.0 {
        return Err(WcError::Revenue);
    }
    if req.purchases == 0.0 {
        return Err(WcError::Purchases);
    }
    if req.cogs == 0.0 {
        return Err(WcError::Cogs);
    }
    if req.accounts_payable == 0.0 {
        return Err(WcError::AccountsPayable);
    }
    let dso = req.accounts_receivable / req.revenue * req.days_analysis;
    let dpo = req.accounts_payable / req.purchases * req.days_analysis;
    let dio = req.inventory / req.cogs * req.days_analysis;
    let ccc = dso + dio - dpo;

    let mut recommendations = Vec::new();
    recommendations.push((
        "DSO".to_string(),
        if dso > 60.0 {
            "Consider tighter credit policies, early payment discounts, or invoice factoring"
        } else {
            "DSO is within acceptable range"
        }
        .to_string(),
    ));
    recommendations.push((
        "DPO".to_string(),
        if dpo < 45.0 {
            "Negotiate extended payment terms with suppliers to improve cash flow"
        } else {
            "DPO is well managed"
        }
        .to_string(),
    ));
    recommendations.push((
        "DIO".to_string(),
        if dio > 90.0 {
            "Implement just-in-time inventory, review slow-moving stock, improve demand forecasting"
        } else {
            "Inventory turnover is satisfactory"
        }
        .to_string(),
    ));

    // NOTE: current assets here are receivables plus inventory only.
    let current_assets = req.accounts_receivable + req.inventory;
    let working_capital = current_assets - req.accounts_payable;

    Ok(WcOptimization {
        company_id: req.company_id.clone(),
        dso: round1(dso),
        dpo: round1(dpo),
        dio: round1(dio),
        cash_conversion_cycle: round1(ccc),
        working_capital,
        current_ratio: py_round2(current_assets / req.accounts_payable),
        quick_ratio: py_round2(req.accounts_receivable / req.accounts_payable),
        net_working_capital: working_capital,
        recommendations,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn analysis_input() -> WcAnalysisInput {
        WcAnalysisInput {
            company_id: "c1".into(),
            current_assets: 500_000.0,
            current_liabilities: 300_000.0,
            inventory: 120_000.0,
            cash: 50_000.0,
            accounts_receivable: 200_000.0,
            accounts_payable: 150_000.0,
            revenue: 1_200_000.0,
            cost_of_goods_sold: 900_000.0,
        }
    }

    #[test]
    fn analysis_metrics_and_cycle() {
        let r = analyze_working_capital(&analysis_input());
        assert_eq!(r.metrics.working_capital, 200_000.0);
        assert_eq!(r.metrics.nwc, 150_000.0);
        assert_eq!(r.metrics.current_ratio, 1.6667);
        assert_eq!(r.metrics.quick_ratio, 1.2667);
        assert_eq!(r.metrics.cash_ratio, 0.1667);
        assert_eq!(r.inventory_turnover, 7.5);
        assert_eq!(r.receivables_turnover, 6.0);
        assert_eq!(r.payables_turnover, 6.0);
        assert_eq!(r.metrics.operating_cycle_days, 109.5);
        assert_eq!(r.cash_conversion_cycle, 48.67);
        assert_eq!(r.working_capital_turnover, 6.0);
        assert!(r.recommendations.is_empty());
    }

    #[test]
    fn analysis_triggers_all_three_warnings() {
        let r = analyze_working_capital(&WcAnalysisInput {
            current_assets: 100_000.0,
            current_liabilities: 100_000.0,
            inventory: 80_000.0,
            accounts_receivable: 400_000.0,
            revenue: 500_000.0,
            ..analysis_input()
        });
        assert_eq!(r.metrics.current_ratio, 1.0);
        assert!(r.cash_conversion_cycle > 90.0);
        assert_eq!(r.recommendations.len(), 3);
    }

    #[test]
    fn analysis_guards_every_denominator() {
        let r = analyze_working_capital(&WcAnalysisInput {
            inventory: 0.0,
            accounts_receivable: 0.0,
            accounts_payable: 0.0,
            current_liabilities: 0.0,
            ..analysis_input()
        });
        assert_eq!(r.metrics.current_ratio, 0.0);
        assert_eq!(r.inventory_turnover, 0.0);
        assert_eq!(r.cash_conversion_cycle, 0.0);
        // zero liabilities leave capital intact, so turnover is measured on it
        assert_eq!(r.metrics.working_capital, 500_000.0);
        assert_eq!(r.working_capital_turnover, 2.4);
    }

    fn finance_input() -> WcFinanceInput {
        WcFinanceInput {
            company_id: "c1".into(),
            period: "2026".into(),
            current_assets: 600_000.0,
            current_liabilities: 300_000.0,
            inventory: 150_000.0,
            accounts_receivable: 200_000.0,
            accounts_payable: 120_000.0,
            annual_revenue: 1_200_000.0,
            cogs: 900_000.0,
        }
    }

    #[test]
    fn finance_cycle_and_recommendation() {
        let r = analyze_wc_finance(&finance_input());
        assert_eq!(r.working_capital, 300_000.0);
        assert_eq!(r.current_ratio, 2.0);
        assert_eq!(r.quick_ratio, 1.5);
        assert_eq!(r.dso, 60.8);
        assert_eq!(r.dio, 60.8);
        assert_eq!(r.dpo, 48.7);
        assert_eq!(r.cash_conversion_cycle, 73.0);
        assert_eq!(r.financing_need, 0.0);
        assert!(r.recommendation.starts_with("Strong working capital"));
    }

    #[test]
    fn finance_deficit_bands() {
        let r = analyze_wc_finance(&WcFinanceInput {
            current_assets: 200_000.0,
            current_liabilities: 300_000.0,
            ..finance_input()
        });
        assert_eq!(r.financing_need, 100_000.0);
        assert!(r.recommendation.starts_with("Working capital deficit"));
        let mid = analyze_wc_finance(&WcFinanceInput {
            current_assets: 400_000.0,
            ..finance_input()
        });
        assert!(mid.recommendation.starts_with("Adequate but monitor"));
    }

    #[test]
    fn factoring_annualises_over_sixty_days() {
        let r = calculate_factoring(&FactoringInput {
            invoice_amount: 100_000.0,
            advance_rate: 0.8,
            factor_fee_rate: 0.02,
            discount_rate: 0.01,
        });
        assert_eq!(r.advance_amount, 80_000.0);
        assert_eq!(r.fee, 2_000.0);
        assert_eq!(r.discount, 1_000.0);
        assert_eq!(r.net_to_company, 78_000.0);
        assert_eq!(r.total_cost, 3_000.0);
        assert_eq!(r.effective_annual_rate, 23.7);
    }

    #[test]
    fn factoring_rate_guarded_when_advance_below_cost() {
        let r = calculate_factoring(&FactoringInput {
            invoice_amount: 100_000.0,
            advance_rate: 0.02,
            factor_fee_rate: 0.05,
            discount_rate: 0.01,
        });
        assert_eq!(r.effective_annual_rate, 0.0);
    }

    fn optimize_input() -> WcOptimizeInput {
        WcOptimizeInput {
            company_id: "c1".into(),
            accounts_receivable: 300_000.0,
            accounts_payable: 150_000.0,
            inventory: 400_000.0,
            revenue: 1_500_000.0,
            purchases: 900_000.0,
            cogs: 1_000_000.0,
            days_analysis: 365.0,
        }
    }

    #[test]
    fn optimization_dso_dio_dpo() {
        let r = optimize_working_capital(&optimize_input()).unwrap();
        assert_eq!(r.dso, 73.0);
        assert_eq!(r.dio, 146.0);
        assert_eq!(r.dpo, 60.8);
        assert_eq!(r.cash_conversion_cycle, 158.2);
        assert_eq!(r.working_capital, 550_000.0);
        assert_eq!(r.current_ratio, 4.67);
        assert_eq!(r.quick_ratio, 2.0);
        assert_eq!(r.recommendations.len(), 3);
        assert!(r.recommendations[0]
            .1
            .starts_with("Consider tighter credit"));
        assert!(r.recommendations[2].1.starts_with("Implement just-in-time"));
    }

    #[test]
    fn optimization_short_cycle_advice() {
        let r = optimize_working_capital(&WcOptimizeInput {
            accounts_receivable: 100_000.0,
            inventory: 100_000.0,
            ..optimize_input()
        })
        .unwrap();
        assert!(r.recommendations[0].1.contains("acceptable range"));
        assert!(r.recommendations[2].1.contains("satisfactory"));
    }

    #[test]
    fn optimization_errors_instead_of_dividing_by_zero() {
        let rev = optimize_working_capital(&WcOptimizeInput {
            revenue: 0.0,
            ..optimize_input()
        })
        .unwrap_err();
        assert_eq!(rev, WcError::Revenue);
        let pur = optimize_working_capital(&WcOptimizeInput {
            purchases: 0.0,
            ..optimize_input()
        })
        .unwrap_err();
        assert_eq!(pur, WcError::Purchases);
        let cogs = optimize_working_capital(&WcOptimizeInput {
            cogs: 0.0,
            ..optimize_input()
        })
        .unwrap_err();
        assert_eq!(cogs, WcError::Cogs);
        let e = optimize_working_capital(&WcOptimizeInput {
            accounts_payable: 0.0,
            ..optimize_input()
        })
        .unwrap_err();
        assert_eq!(e.field(), "accounts_payable");
        assert_eq!(e.to_string(), "accounts_payable must be non-zero");
    }
}
