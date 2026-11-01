//! ratio-analysis-service `/ratios`: liquidity, profitability, leverage,
//! efficiency and valuation ratios with the Python overall-score formula.
use serde::{Deserialize, Serialize};
use vimbai_common::{py_round, py_round2};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BalanceSheet {
    pub current_assets: f64,
    pub current_liabilities: f64,
    pub total_assets: f64,
    pub total_debt: f64,
    pub total_equity: f64,
    pub inventory: f64,
    pub accounts_receivable: f64,
    pub accounts_payable: f64,
    pub cash: f64,
    pub marketable_securities: f64,
    pub market_cap: Option<f64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IncomeStatement {
    pub revenue: f64,
    pub net_income: f64,
    pub ebit: Option<f64>,
    pub cogs: f64,
    pub interest_expense: Option<f64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RatioInput {
    pub company_id: String,
    pub period: String,
    pub balance_sheet: BalanceSheet,
    pub income_statement: IncomeStatement,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Liquidity {
    pub current_ratio: f64,
    pub quick_ratio: f64,
    pub cash_ratio: f64,
    pub working_capital: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Profitability {
    pub gross_margin: f64,
    pub operating_margin: f64,
    pub net_margin: f64,
    pub roe: f64,
    pub roa: f64,
    pub roce: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Leverage {
    pub debt_to_equity: f64,
    pub debt_to_assets: f64,
    pub interest_coverage: f64,
    pub equity_multiplier: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Efficiency {
    pub asset_turnover: f64,
    pub inventory_turnover: f64,
    pub ar_turnover: f64,
    pub ap_turnover: f64,
    pub days_sales_outstanding: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Valuation {
    pub pe_ratio: f64,
    pub pb_ratio: f64,
    pub ps_ratio: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RatioOutput {
    pub company_id: String,
    pub period: String,
    pub liquidity: Liquidity,
    pub profitability: Profitability,
    pub leverage: Leverage,
    pub efficiency: Efficiency,
    pub valuation: Valuation,
    pub overall_score: f64,
    pub recommendations: Vec<String>,
}

fn div(num: f64, den: f64) -> f64 {
    if den == 0.0 {
        0.0
    } else {
        num / den
    }
}

pub fn calculate_ratios(input: &RatioInput) -> RatioOutput {
    let bs = &input.balance_sheet;
    let is = &input.income_statement;
    let current_assets = bs.current_assets;
    let current_liabilities = bs.current_liabilities;
    // PYTHON QUIRK: total_assets defaults to 1 when omitted (0 in Rust),
    // so a zero balance sheet still divides instead of returning 0.
    let total_assets = if bs.total_assets == 0.0 {
        1.0
    } else {
        bs.total_assets
    };
    let total_debt = bs.total_debt;
    let equity = bs.total_equity;
    let revenue = is.revenue;
    let net_income = is.net_income;
    let ebit = is.ebit.unwrap_or(net_income);
    let cogs = is.cogs;

    let liquidity = Liquidity {
        current_ratio: py_round(div(current_assets, current_liabilities), 4),
        quick_ratio: py_round(div(current_assets - bs.inventory, current_liabilities), 4),
        cash_ratio: py_round(
            div(bs.cash + bs.marketable_securities, current_liabilities),
            4,
        ),
        working_capital: py_round2(current_assets - current_liabilities),
    };

    let profitability = Profitability {
        gross_margin: py_round2(div((revenue - cogs) * 100.0, revenue)),
        operating_margin: py_round2(div(ebit * 100.0, revenue)),
        net_margin: py_round2(div(net_income * 100.0, revenue)),
        roe: py_round2(div(net_income * 100.0, equity)),
        roa: py_round2(div(net_income * 100.0, total_assets)),
        roce: py_round2(div(ebit * 100.0, total_assets - current_liabilities)),
    };

    // interest_expense defaults to 1 and `or 1` treats 0 as missing.
    let interest = match is.interest_expense {
        Some(v) if v != 0.0 => v,
        _ => 1.0,
    };
    let leverage = Leverage {
        debt_to_equity: py_round(div(total_debt, equity), 4),
        debt_to_assets: py_round(div(total_debt, total_assets), 4),
        interest_coverage: py_round2(ebit / interest),
        equity_multiplier: py_round(div(total_assets, equity), 4),
    };

    let efficiency = Efficiency {
        asset_turnover: py_round(div(revenue, total_assets), 4),
        inventory_turnover: py_round(div(cogs, bs.inventory), 4),
        ar_turnover: py_round(div(revenue, bs.accounts_receivable), 4),
        ap_turnover: py_round(div(cogs, bs.accounts_payable), 4),
        days_sales_outstanding: py_round2(div(bs.accounts_receivable * 365.0, revenue)),
    };

    let market_cap = bs.market_cap.unwrap_or(equity * 3.0);
    let valuation = Valuation {
        pe_ratio: py_round2(div(market_cap, net_income)),
        pb_ratio: py_round2(div(market_cap, equity)),
        ps_ratio: py_round2(div(market_cap, revenue)),
    };

    let overall_score = ((liquidity.current_ratio / 2.0).min(100.0)
        + (profitability.net_margin + 50.0).min(100.0)
        + (100.0 - leverage.debt_to_equity * 20.0).min(100.0)
        + (efficiency.asset_turnover * 50.0).min(100.0))
        / 4.0;

    let mut recommendations = Vec::new();
    if liquidity.current_ratio < 1.5 {
        recommendations.push("Improve liquidity position".to_string());
    }
    if profitability.net_margin < 10.0 {
        recommendations.push("Focus on cost reduction".to_string());
    }
    if leverage.debt_to_equity > 2.0 {
        recommendations.push("Consider debt reduction".to_string());
    }

    RatioOutput {
        company_id: input.company_id.clone(),
        period: input.period.clone(),
        liquidity,
        profitability,
        leverage,
        efficiency,
        valuation,
        overall_score: py_round2(overall_score),
        recommendations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> RatioInput {
        RatioInput {
            company_id: "c1".into(),
            period: "2026".into(),
            balance_sheet: BalanceSheet {
                current_assets: 300_000.0,
                current_liabilities: 150_000.0,
                total_assets: 1_000_000.0,
                total_debt: 400_000.0,
                total_equity: 600_000.0,
                inventory: 100_000.0,
                accounts_receivable: 120_000.0,
                accounts_payable: 90_000.0,
                cash: 50_000.0,
                marketable_securities: 25_000.0,
                market_cap: None,
            },
            income_statement: IncomeStatement {
                revenue: 800_000.0,
                net_income: 120_000.0,
                ebit: None,
                cogs: 500_000.0,
                interest_expense: None,
            },
        }
    }

    #[test]
    fn liquidity_block() {
        let r = calculate_ratios(&input());
        assert_eq!(r.liquidity.current_ratio, 2.0);
        assert_eq!(r.liquidity.quick_ratio, 1.3333);
        assert_eq!(r.liquidity.cash_ratio, 0.5);
        assert_eq!(r.liquidity.working_capital, 150_000.0);
    }

    #[test]
    fn profitability_block() {
        let r = calculate_ratios(&input());
        assert_eq!(r.profitability.gross_margin, 37.5);
        assert_eq!(r.profitability.operating_margin, 15.0);
        assert_eq!(r.profitability.net_margin, 15.0);
        assert_eq!(r.profitability.roe, 20.0);
        assert_eq!(r.profitability.roa, 12.0);
        // ebit / (assets - current liabilities)
        assert_eq!(r.profitability.roce, 14.12);
    }

    #[test]
    fn leverage_interest_falls_back_to_one() {
        let r = calculate_ratios(&input());
        assert_eq!(r.leverage.debt_to_equity, 0.6667);
        assert_eq!(r.leverage.interest_coverage, 120_000.0);
        assert_eq!(r.leverage.equity_multiplier, 1.6667);
    }

    #[test]
    fn efficiency_and_valuation() {
        let r = calculate_ratios(&input());
        assert_eq!(r.efficiency.asset_turnover, 0.8);
        assert_eq!(r.efficiency.inventory_turnover, 5.0);
        assert_eq!(r.efficiency.ap_turnover, 5.5556);
        assert_eq!(r.efficiency.days_sales_outstanding, 54.75);
        // market cap defaults to equity * 3
        assert_eq!(r.valuation.pe_ratio, 15.0);
        assert_eq!(r.valuation.pb_ratio, 3.0);
        assert_eq!(r.valuation.ps_ratio, 2.25);
    }

    #[test]
    fn score_and_recommendations() {
        let r = calculate_ratios(&input());
        // min(2/2,100)=1 + min(65,100)=65 + min(100-13.33,100)=86.67 + min(40,100)=40
        assert!(
            (r.overall_score - 48.17).abs() < 0.02,
            "{}",
            r.overall_score
        );
        assert!(r.recommendations.is_empty());
    }
}
