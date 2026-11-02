//! financial-reporting-service: the three statements plus the four metrics a
//! reader asks for first.
//!
//! PYTHON QUIRK: equity is derived, never read, so a balance sheet that does
//! not balance still reports a total. The current ratio falls back to a
//! liability of one, so a missing denominator reads as the asset total rather
//! than failing.
use serde::{Deserialize, Serialize};
use serde_json::Map;
use vimbai_common::py_round2;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FinancialReportingRequest {
    pub company_id: String,
    #[serde(default)]
    pub period: String,
    #[serde(default)]
    pub balance_sheet: Map<String, serde_json::Value>,
    #[serde(default)]
    pub income_statement: Map<String, serde_json::Value>,
    #[serde(default)]
    pub cash_flow: Map<String, serde_json::Value>,
}

fn value(map: &Map<String, serde_json::Value>, key: &str, fallback: f64) -> f64 {
    map.get(key).and_then(|v| v.as_f64()).unwrap_or(fallback)
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct KeyMetrics {
    pub roe: f64,
    pub roa: f64,
    pub current_ratio: f64,
    pub debt_to_equity: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FinancialReport {
    pub company_id: String,
    pub report_date: String,
    pub period: String,
    /// Balance sheet, income statement and cash flow, each with the totals and
    /// margins the service folds in.
    pub financial_statements: Map<String, serde_json::Value>,
    pub key_metrics: KeyMetrics,
    pub report_status: String,
    pub recommendations: Vec<String>,
}

/// `/generate`. `report_date` comes from the caller; the service stamps now.
pub fn generate_financial_report(
    req: &FinancialReportingRequest,
    report_date: &str,
) -> FinancialReport {
    let bs = &req.balance_sheet;
    let is = &req.income_statement;
    let current_assets = value(bs, "current_assets", 0.0);
    let total_assets = current_assets + value(bs, "non_current_assets", 0.0);
    let total_liabilities =
        value(bs, "current_liabilities", 0.0) + value(bs, "non_current_liabilities", 0.0);
    let equity = total_assets - total_liabilities;

    let revenue = value(is, "revenue", 0.0);
    let net_income = value(is, "net_income", 0.0);

    let mut balance_sheet = bs.clone();
    balance_sheet.insert("total_assets".to_string(), py_round2(total_assets).into());
    balance_sheet.insert(
        "total_liabilities".to_string(),
        py_round2(total_liabilities).into(),
    );
    balance_sheet.insert("total_equity".to_string(), py_round2(equity).into());

    let mut income_statement = is.clone();
    income_statement.insert(
        "gross_margin".to_string(),
        if revenue != 0.0 {
            py_round2((revenue - value(is, "cogs", 0.0)) / revenue * 100.0)
        } else {
            0.0
        }
        .into(),
    );
    income_statement.insert(
        "net_margin".to_string(),
        if revenue != 0.0 {
            py_round2(net_income / revenue * 100.0)
        } else {
            0.0
        }
        .into(),
    );

    let mut statements = Map::new();
    statements.insert(
        "balance_sheet".to_string(),
        serde_json::Value::Object(balance_sheet),
    );
    statements.insert(
        "income_statement".to_string(),
        serde_json::Value::Object(income_statement),
    );
    statements.insert(
        "cash_flow".to_string(),
        serde_json::Value::Object(req.cash_flow.clone()),
    );

    let key_metrics = KeyMetrics {
        roe: if equity != 0.0 {
            py_round2(net_income / equity * 100.0)
        } else {
            0.0
        },
        roa: if total_assets != 0.0 {
            py_round2(net_income / total_assets * 100.0)
        } else {
            0.0
        },
        // a missing current liability falls back to one
        current_ratio: py_round2(current_assets / value(bs, "current_liabilities", 1.0)),
        debt_to_equity: if equity != 0.0 {
            py_round2(total_liabilities / equity)
        } else {
            0.0
        },
    };

    let mut recommendations = Vec::new();
    if key_metrics.roe < 10.0 {
        recommendations.push("Low ROE - improve profitability or efficiency".to_string());
    }
    if key_metrics.current_ratio < 1.5 {
        recommendations.push("Low liquidity - improve working capital".to_string());
    }

    FinancialReport {
        company_id: req.company_id.clone(),
        report_date: report_date.to_string(),
        period: req.period.clone(),
        financial_statements: statements,
        key_metrics,
        report_status: "Ready".to_string(),
        recommendations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn request() -> FinancialReportingRequest {
        serde_json::from_value(json!({
            "company_id": "c1",
            "period": "2026",
            "balance_sheet": {"current_assets": 400000, "non_current_assets": 1600000,
                              "current_liabilities": 250000, "non_current_liabilities": 500000},
            "income_statement": {"revenue": 2000000, "cogs": 1100000, "net_income": 150000},
            "cash_flow": {"operating": 300000, "investing": -100000, "financing": -50000}
        }))
        .unwrap()
    }

    #[test]
    fn equity_is_derived_rather_than_read() {
        let report = generate_financial_report(&request(), "2026-11-02");
        let balance = &report.financial_statements["balance_sheet"];
        assert_eq!(balance["total_assets"], 2_000_000.0);
        assert_eq!(balance["total_liabilities"], 750_000.0);
        assert_eq!(balance["total_equity"], 1_250_000.0);
        assert_eq!(report.key_metrics.debt_to_equity, 0.6);
    }

    #[test]
    fn the_metrics_follow_from_the_statement_totals() {
        let metrics = generate_financial_report(&request(), "x").key_metrics;
        assert_eq!(metrics.roe, 12.0);
        assert_eq!(metrics.roa, 7.5);
        assert_eq!(metrics.current_ratio, 1.6);
    }

    #[test]
    fn a_thin_return_and_tight_liquidity_are_both_flagged() {
        let report = generate_financial_report(&request(), "x");
        assert!(report.recommendations.is_empty());
        let weak = generate_financial_report(
            &serde_json::from_value(json!({
                "company_id": "c1",
                "balance_sheet": {"current_assets": 100, "current_liabilities": 100},
                "income_statement": {"revenue": 1000, "net_income": 5},
                "cash_flow": {}
            }))
            .unwrap(),
            "x",
        );
        assert_eq!(weak.recommendations.len(), 2);
        assert_eq!(weak.key_metrics.current_ratio, 1.0);
    }

    #[test]
    fn the_margins_ride_on_the_income_statement() {
        let statements = generate_financial_report(&request(), "x").financial_statements;
        assert_eq!(statements["income_statement"]["gross_margin"], 45.0);
        assert_eq!(statements["income_statement"]["net_margin"], 7.5);
        // the live figures keep their places next to the folded-in ones
        assert_eq!(statements["income_statement"]["revenue"], 2_000_000.0);
        assert_eq!(statements["cash_flow"]["operating"], 300_000.0);
    }

    #[test]
    fn an_empty_statement_does_not_divide() {
        let report = generate_financial_report(&FinancialReportingRequest::default(), "x");
        assert_eq!(report.key_metrics.roe, 0.0);
        assert_eq!(report.key_metrics.roa, 0.0);
        // no current liabilities means the fallback of one applies
        assert_eq!(report.key_metrics.current_ratio, 0.0);
        assert_eq!(report.report_status, "Ready");
    }
}
