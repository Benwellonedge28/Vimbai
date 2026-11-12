//! financial-statements-service `/prepare`: derived balance sheet,
//! income statement, key ratios and health score.
//!
//! Every figure on the two statements is a `Money`. Assets, liabilities, equity, revenue, net income
//! and EBIT are amounts, so a company whose balance sheet runs past what a double can hold still
//! balances exactly. The published ratios stay ratios: they are amounts divided by amounts, and the
//! service rounded them to two places before answering.
//!
//! Two behaviours were corrected and are pinned by tests:
//!
//! * The service defaulted a *missing* current asset or liability to 1, which is a sensible ratio for
//!   an absent figure, but a *present* zero divided by it raised `ZeroDivisionError` and returned 500.
//!   A zero denominator now reports a ratio of 0: an undefined current ratio is not a healthy one.
//! * The health score is computed from the ratios as published (debt to equity and ROE rounded to two
//!   places, current ratio and margin as raw values), exactly as the service did, so the two never
//!   disagree with the figures printed beside them.
use serde::{Deserialize, Serialize};
use vimbai_common::{py_round2, Money};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StatementsInput {
    pub company_id: String,
    pub period: String,
    pub current_assets: Money,
    pub non_current_assets: Money,
    pub current_liabilities: Money,
    pub non_current_liabilities: Money,
    pub revenue: Money,
    pub net_income: Money,
    pub ebitda: Money,
    pub depreciation: Money,
    pub cogs: Money,
    pub inventory: Money,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StatementsOutput {
    pub company_id: String,
    pub period: String,
    pub total_assets: Money,
    pub total_liabilities: Money,
    pub equity: Money,
    pub revenue: Money,
    pub net_income: Money,
    pub ebit: Money,
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

/// One amount divided by another as a ratio, the way the service published these figures.
fn ratio(numerator: &Money, denominator: &Money) -> f64 {
    if denominator.is_zero() {
        return 0.0;
    }
    numerator.over(denominator).to_f64_lossy()
}

/// An amount as a percentage of another, which is a ratio scaled by a hundred.
fn percent(part: &Money, whole: &Money) -> f64 {
    ratio(part, whole) * 100.0
}

pub fn prepare_statements(input: &StatementsInput) -> StatementsOutput {
    let total_assets = input.current_assets.plus(&input.non_current_assets);
    let total_liabilities = input
        .current_liabilities
        .plus(&input.non_current_liabilities);
    let equity = total_assets.minus(&total_liabilities);
    let revenue = input.revenue.clone();
    let net_income = input.net_income.clone();
    let ebit = input.ebitda.minus(&input.depreciation);

    let current_ratio = ratio(&input.current_assets, &input.current_liabilities);
    let quick_ratio = ratio(
        &input.current_assets.minus(&input.inventory),
        &input.current_liabilities,
    );
    let roe = percent(&net_income, &equity);
    let roa = percent(&net_income, &total_assets);
    let profit_margin = percent(&net_income, &revenue);
    let debt_to_equity = py_round2(ratio(&total_liabilities, &equity));
    let gross_margin = py_round2(percent(&revenue.minus(&input.cogs), &revenue));

    let roe_published = py_round2(roe);
    let health_score = ((current_ratio / 2.0 * 25.0)
        + (profit_margin / 20.0 * 25.0)
        + ((100.0 - debt_to_equity.abs()) * 0.25)
        + (roe_published / 30.0 * 25.0))
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
        debt_to_equity,
        roe: roe_published,
        roa: py_round2(roa),
        profit_margin: py_round2(profit_margin),
        gross_margin,
        financial_health_score: py_round2(health_score),
        recommendations,
    }
}

/// The service's `/prepare` body, and its nested response.
///
/// The endpoint took four dictionaries and published four back. The calculator works on named
/// components, so the door maps the balance sheet and income statement into them and echoes the cash
/// flow and equity statement through untouched, which is what the service did with them.
pub fn prepare_statements_from_json(
    payload: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    #[derive(Deserialize, Default)]
    struct BalanceSheet {
        #[serde(default)]
        current_assets: Money,
        #[serde(default)]
        non_current_assets: Money,
        #[serde(default)]
        current_liabilities: Money,
        #[serde(default)]
        non_current_liabilities: Money,
        #[serde(default)]
        inventory: Money,
    }
    #[derive(Deserialize, Default)]
    struct IncomeStatement {
        #[serde(default)]
        revenue: Money,
        #[serde(default)]
        net_income: Money,
        #[serde(default)]
        ebitda: Money,
        #[serde(default)]
        depreciation: Money,
        #[serde(default)]
        cogs: Money,
    }
    #[derive(Deserialize)]
    struct Request {
        company_id: String,
        period: String,
        #[serde(default)]
        balance_sheet: Option<BalanceSheet>,
        #[serde(default)]
        income_statement: Option<IncomeStatement>,
        #[serde(default)]
        cash_flow: Option<serde_json::Value>,
        #[serde(default)]
        equity_statement: Option<serde_json::Value>,
    }

    let request: Request = serde_json::from_value(payload.clone())
        .map_err(|error| format!("invalid request: {error}"))?;
    let balance = request.balance_sheet.unwrap_or_default();
    let income = request.income_statement.unwrap_or_default();

    let output = prepare_statements(&StatementsInput {
        company_id: request.company_id.clone(),
        period: request.period.clone(),
        current_assets: balance.current_assets,
        non_current_assets: balance.non_current_assets,
        current_liabilities: balance.current_liabilities,
        non_current_liabilities: balance.non_current_liabilities,
        revenue: income.revenue,
        net_income: income.net_income,
        ebitda: income.ebitda,
        depreciation: income.depreciation,
        cogs: income.cogs,
        inventory: balance.inventory,
    });

    Ok(serde_json::json!({
        "company_id": output.company_id,
        "period": output.period,
        "statements": {
            "balance_sheet": {
                "total_assets": output.total_assets,
                "total_liabilities": output.total_liabilities,
                "equity": output.equity,
            },
            "income_statement": {
                "revenue": output.revenue,
                "net_income": output.net_income,
                "ebit": output.ebit,
            },
            "cash_flow": request.cash_flow.unwrap_or_else(|| serde_json::json!({})),
            "equity_statement": request.equity_statement.unwrap_or_else(|| serde_json::json!({})),
        },
        "key_ratios": {
            "current_ratio": output.current_ratio,
            "quick_ratio": output.quick_ratio,
            "debt_to_equity": output.debt_to_equity,
            "roe": output.roe,
            "roa": output.roa,
            "profit_margin": output.profit_margin,
            "gross_margin": output.gross_margin,
        },
        "financial_health_score": output.financial_health_score,
        "recommendations": output.recommendations,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn money(text: &str) -> Money {
        Money::parse(text).expect("amount")
    }

    fn input() -> StatementsInput {
        StatementsInput {
            company_id: "c1".into(),
            period: "2026".into(),
            current_assets: money("400000"),
            non_current_assets: money("600000"),
            current_liabilities: money("200000"),
            non_current_liabilities: money("300000"),
            revenue: money("1000000"),
            net_income: money("150000"),
            ebitda: money("260000"),
            depreciation: money("60000"),
            cogs: money("600000"),
            inventory: money("100000"),
        }
    }

    #[test]
    fn statements_derived_from_components() {
        let r = prepare_statements(&input());
        assert_eq!(r.total_assets.to_fixed(2), "1000000.00");
        assert_eq!(r.total_liabilities.to_fixed(2), "500000.00");
        assert_eq!(r.equity.to_fixed(2), "500000.00");
        assert_eq!(r.ebit.to_fixed(2), "200000.00");
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
            current_assets: money("100000"),
            current_liabilities: money("100000"),
            non_current_assets: money("400000"),
            non_current_liabilities: money("350000"),
            revenue: money("500000"),
            net_income: money("2000"),
            ebitda: money("2000"),
            depreciation: Money::zero(),
            cogs: money("450000"),
            inventory: money("50000"),
            ..input()
        });
        // equity 50k -> d/e 9.0 and roe 4%
        assert_eq!(r.equity.to_fixed(2), "50000.00");
        assert_eq!(r.debt_to_equity, 9.0);
        assert_eq!(r.roe, 4.0);
        assert_eq!(r.recommendations.len(), 3);
        assert!(r.recommendations[0].starts_with("Low liquidity"));
    }

    #[test]
    fn a_balance_sheet_past_double_precision_still_balances() {
        let mut i = input();
        i.current_assets = money("123456789012345678901.55");
        i.non_current_assets = money("0.45");
        i.current_liabilities = money("123456789012345678902");
        i.non_current_liabilities = Money::zero();
        let r = prepare_statements(&i);
        assert_eq!(r.total_assets.to_fixed(2), "123456789012345678902.00");
        assert_eq!(r.total_liabilities.to_fixed(2), "123456789012345678902.00");
        assert_eq!(r.equity.to_fixed(2), "0.00");
    }

    #[test]
    fn a_zero_denominator_reports_a_ratio_rather_than_crashing() {
        // The service raised ZeroDivisionError here and answered 500.
        let i = StatementsInput {
            current_assets: Money::zero(),
            current_liabilities: Money::zero(),
            ..input()
        };
        let r = prepare_statements(&i);
        assert_eq!(r.current_ratio, 0.0);
        assert!(r.recommendations[0].starts_with("Low liquidity"));
    }

    #[test]
    fn the_door_publishes_the_nested_shape_it_was_given() {
        let payload = serde_json::json!({
            "company_id": "c1",
            "period": "2026",
            "balance_sheet": {
                "current_assets": "400000", "non_current_assets": "600000",
                "current_liabilities": "200000", "non_current_liabilities": "300000",
                "inventory": "100000"
            },
            "income_statement": {
                "revenue": "1000000", "net_income": "150000", "ebitda": "260000",
                "depreciation": "60000", "cogs": "600000"
            },
            "cash_flow": {"operating": "120000"},
            "equity_statement": {"dividends": "10000"}
        });
        let out = prepare_statements_from_json(&payload).expect("door");
        assert_eq!(out["statements"]["balance_sheet"]["equity"], "500000");
        assert_eq!(out["statements"]["income_statement"]["ebit"], "200000");
        assert_eq!(out["statements"]["cash_flow"]["operating"], "120000");
        assert_eq!(out["key_ratios"]["current_ratio"], serde_json::json!(2.0));
        assert_eq!(out["financial_health_score"], serde_json::json!(93.5));
    }

    #[test]
    fn a_missing_statement_is_treated_as_empty_not_as_an_error() {
        let payload = serde_json::json!({"company_id": "c1", "period": "2026"});
        let out = prepare_statements_from_json(&payload).expect("door");
        assert_eq!(out["statements"]["balance_sheet"]["total_assets"], "0");
        assert_eq!(out["key_ratios"]["roe"], serde_json::json!(0.0));
    }
}
