//! financial-statements-service `/prepare`: derived balance sheet,
//! income statement, key ratios and health score.
use serde::{Deserialize, Serialize};
use vimbai_common::py_round2;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StatementsInput {
    pub company_id: String,
    pub period: String,
    pub current_assets: f64,
    pub non_current_assets: f64,
    pub current_liabilities: f64,
    pub non_current_liabilities: f64,
    pub revenue: f64,
    pub net_income: f64,
    pub ebitda: f64,
    pub depreciation: f64,
    pub cogs: f64,
    pub inventory: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StatementsOutput {
    pub company_id: String,
    pub period: String,
    pub total_assets: f64,
    pub total_liabilities: f64,
    pub equity: f64,
    pub revenue: f64,
    pub net_income: f64,
    pub ebit: f64,
    pub current_ratio: f64,
    pub quick_ratio: f64,
    pub debt_to_equity: f64,
    pub roe: f64,
    pub roa: f64,
    pub profit_margin: f64,
    pub gross_margin: f64,
    pub financial_health_score: f64,
    pub recommendations: Vec<String>,
}

fn div(num: f64, den: f64) -> f64 {
    if den == 0.0 {
        0.0
    } else {
        num / den
    }
}

pub fn prepare_statements(input: &StatementsInput) -> StatementsOutput {
    let total_assets = input.current_assets + input.non_current_assets;
    let total_liabilities = input.current_liabilities + input.non_current_liabilities;
    let equity = total_assets - total_liabilities;
    let revenue = input.revenue;
    let net_income = input.net_income;
    let ebit = input.ebitda - input.depreciation;
    // PYTHON QUIRK: both divisions fall back to 1, not 0, when missing.
    let cur_assets = if input.current_assets == 0.0 {
        1.0
    } else {
        input.current_assets
    };
    let cur_liab = if input.current_liabilities == 0.0 {
        1.0
    } else {
        input.current_liabilities
    };
    let current_ratio = cur_assets / cur_liab;
    let quick_ratio = (input.current_assets - input.inventory) / cur_liab;
    let roe = div(net_income * 100.0, equity);
    let roa = div(net_income * 100.0, total_assets);
    let profit_margin = div(net_income * 100.0, revenue);
    let debt_to_equity = div(total_liabilities, equity);
    let gross_margin = div((revenue - input.cogs) * 100.0, revenue);

    let health_score = ((current_ratio / 2.0 * 25.0)
        + (profit_margin / 20.0 * 25.0)
        + ((100.0 - debt_to_equity.abs()) * 0.25)
        + (roe / 30.0 * 25.0))
        .min(100.0);

    let mut recommendations = Vec::new();
    if current_ratio < 1.5 {
        recommendations.push("Low liquidity - improve working capital".to_string());
    }
    if roe < 10.0 {
        recommendations.push("Low return on equity - improve profitability".to_string());
    }
    if debt_to_equity > 2.0 {
        recommendations.push("High leverage - reduce debt levels".to_string());
    }

    StatementsOutput {
        company_id: input.company_id.clone(),
        period: input.period.clone(),
        total_assets,
        total_liabilities,
        equity,
        revenue,
        net_income,
        ebit,
        current_ratio: py_round2(current_ratio),
        quick_ratio: py_round2(quick_ratio),
        debt_to_equity: py_round2(debt_to_equity),
        roe: py_round2(roe),
        roa: py_round2(roa),
        profit_margin: py_round2(profit_margin),
        gross_margin: py_round2(gross_margin),
        financial_health_score: py_round2(health_score),
        recommendations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> StatementsInput {
        StatementsInput {
            company_id: "c1".into(),
            period: "2026".into(),
            current_assets: 400_000.0,
            non_current_assets: 600_000.0,
            current_liabilities: 200_000.0,
            non_current_liabilities: 300_000.0,
            revenue: 1_000_000.0,
            net_income: 150_000.0,
            ebitda: 260_000.0,
            depreciation: 60_000.0,
            cogs: 600_000.0,
            inventory: 100_000.0,
        }
    }

    #[test]
    fn statements_derived_from_components() {
        let r = prepare_statements(&input());
        assert_eq!(r.total_assets, 1_000_000.0);
        assert_eq!(r.total_liabilities, 500_000.0);
        assert_eq!(r.equity, 500_000.0);
        assert_eq!(r.ebit, 200_000.0);
        assert_eq!(r.current_ratio, 2.0);
        assert_eq!(r.quick_ratio, 1.5);
        assert_eq!(r.debt_to_equity, 1.0);
        assert_eq!(r.roe, 30.0);
        assert_eq!(r.roa, 15.0);
        assert_eq!(r.profit_margin, 15.0);
        assert_eq!(r.gross_margin, 40.0);
    }

    #[test]
    fn health_score_and_no_flags_for_healthy_company() {
        let r = prepare_statements(&input());
        // 2/2*25 + 15/20*25 + 99*0.25 + 30/30*25 = 25+18.75+24.75+25
        assert_eq!(r.financial_health_score, 93.5);
        assert!(r.recommendations.is_empty());
    }

    #[test]
    fn flags_fire_on_weak_company() {
        let r = prepare_statements(&StatementsInput {
            current_assets: 100_000.0,
            current_liabilities: 100_000.0,
            non_current_assets: 400_000.0,
            non_current_liabilities: 350_000.0,
            revenue: 500_000.0,
            net_income: 2_000.0,
            ebitda: 2_000.0,
            depreciation: 0.0,
            cogs: 450_000.0,
            inventory: 50_000.0,
            ..input()
        });
        // equity 50k -> d/e 9.0 and roe 4%
        assert_eq!(r.equity, 50_000.0);
        assert_eq!(r.debt_to_equity, 9.0);
        assert_eq!(r.roe, 4.0);
        assert_eq!(r.recommendations.len(), 3);
        assert!(r.recommendations[0].starts_with("Low liquidity"));
    }
}
