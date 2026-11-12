#![forbid(unsafe_code)]
//! Ratio analysis and statement generation.
//!
//! Ported: ratio-analysis, financial-statements, income-statement,
//! balance-sheet, comparative-financial-statements, trading account,
//! profit and loss, management accounts and reporting, working capital
//! family. Queued: consolidation.

pub mod balance_sheet;
pub mod comparative;
pub mod consolidation;
pub mod income_statement;
pub mod interim_reporting;
pub mod investor_metrics;
pub mod ledger_controls;
pub mod management_accounts;
pub mod management_pack;
pub mod ratio_analysis;
pub mod ratios;
pub mod segment_reporting;
pub mod statements;
pub mod trading_account;
pub mod trial_balance;
pub mod working_capital;

pub use balance_sheet::{
    balance_sheet_ratios, generate_totals, BalanceSheetInput, BalanceSheetRatios,
    BalanceSheetTotals,
};
pub use comparative::{
    analyze, generate_periods, horizontal_analysis, horizontal_items, trend_analysis, trend_series,
    variance_analysis, ComparativeAnalysis, ComparativeRequest, HorizontalChange, HorizontalItem,
    PeriodData, RatioComparison, SummaryStatistics, TrendItem, TrendSeries, VarianceLine,
    VarianceReport,
};
pub use consolidation::{
    calculate_goodwill, calculate_nci, consolidate, process_eliminations, ConsolidatedStatement,
    ConsolidationInput, Eliminations, GoodwillCalculation, IntercompanyTransaction, NciCalculation,
    SubFinancials, Subsidiary,
};
pub use income_statement::{prepare_income_statement, IncomeStatementInput, IncomeStatementOutput};
pub use investor_metrics::{
    asset_turnover, asset_turnover_from_json, croic, croic_from_json, du_pont_analysis,
    du_pont_from_json, eps, eps_from_json, eva, eva_from_json, AntiDilutiveItem, CroicMetrics,
    CroicPeriod, CroicRequest, CroicResponse, DuPontMetrics, DuPontRequest, DuPontResponse,
    EpsRequest, EpsResponse, EvaMetrics, EvaRequest, EvaResponse, ShareOption, ShareOptionInput,
    TurnoverMetrics, TurnoverRequest, TurnoverResponse,
};
pub use ledger_controls::{analyze_times_interest_earned, TieAnalysis, TieInput};
pub use management_accounts::{prepare_management_accounts, ManagementAccounts};
pub use ratios::{calculate_ratios, BalanceSheet, IncomeStatement, RatioInput, RatioOutput};
pub use statements::{
    prepare_statements, prepare_statements_from_json, StatementsInput, StatementsOutput,
};
pub use trading_account::{generate_trading_account, TradingAccountInput, TradingAccountResult};
pub use working_capital::{
    analyze_wc_finance, analyze_working_capital, calculate_factoring, optimize_working_capital,
    FactoringInput, FactoringResult, WcAnalysis, WcError, WcFinanceInput, WcFinanceResult,
    WcOptimization, WcOptimizeInput,
};

/// `management-accounts-service /prepare`, from the wire payload.
///
/// The service took its line items as maps; the calculator takes ordered pairs, so the caller's
/// order is preserved and the figures come out in the same sequence.
pub fn management_accounts_from_json(
    payload: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    fn items(payload: &serde_json::Value, key: &str) -> management_accounts::Items {
        payload
            .get(key)
            .and_then(|v| v.as_object())
            .map(|map| {
                map.iter()
                    .map(|(k, v)| (k.clone(), v.as_f64().unwrap_or(0.0)))
                    .collect()
            })
            .unwrap_or_default()
    }
    let text = |key: &str| {
        payload
            .get(key)
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string()
    };
    let accounts = management_accounts::prepare_management_accounts(
        &text("company_id"),
        &text("period"),
        &items(payload, "revenue_items"),
        &items(payload, "cost_items"),
        &items(payload, "balance_sheet_items"),
    );
    serde_json::to_value(accounts).map_err(|err| err.to_string())
}

/// `management-dashboard-service /generate`.
pub fn management_dashboard_from_json(
    payload: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    management_pack::generate_dashboard(payload)
}

/// `financial-planning-service /plan`.
pub fn financial_planning_from_json(
    payload: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    management_pack::create_financial_plan(payload)
}

/// `ratio-analysis-service /ratios`.
pub fn ratio_analysis_ratios_from_json(
    payload: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    ratio_analysis::ratios(payload)
}

/// `ratio-analysis-service /peer-comparison`.
pub fn ratio_analysis_peer_comparison_from_json(
    payload: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    ratio_analysis::peer_comparison(payload)
}

/// `ratio-analysis-service /trend`.
pub fn ratio_analysis_trend_from_json(
    payload: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    ratio_analysis::trend(payload)
}

/// `trial-balance-service /generate`.
pub fn trial_balance_generate_from_json(
    payload: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    trial_balance::generate(payload)
}

/// `trial-balance-service /from-ledgers`.
pub fn trial_balance_from_ledgers_from_json(
    payload: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    trial_balance::generate(payload)
}

/// `trial-balance-service /validate`.
pub fn trial_balance_validate_from_json(
    payload: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    trial_balance::validate(payload)
}

/// `trial-balance-service /classification-summary`.
pub fn trial_balance_classification_summary_from_json(
    payload: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    trial_balance::classification_summary(payload)
}

/// `trial-balance-service /adjusted`.
pub fn trial_balance_adjusted_from_json(
    payload: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    trial_balance::adjusted(payload)
}
