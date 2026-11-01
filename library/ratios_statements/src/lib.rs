#![forbid(unsafe_code)]
//! Ratio analysis and statement generation.
//!
//! Ported: ratio-analysis, financial-statements, income-statement,
//! balance-sheet, comparative-financial-statements, trading account,
//! profit and loss, management accounts and reporting, working capital
//! family. Queued: consolidation.

pub mod balance_sheet;
pub mod comparative;
pub mod income_statement;
pub mod management_accounts;
pub mod management_reporting;
pub mod profit_loss;
pub mod ratios;
pub mod statements;
pub mod trading_account;
pub mod working_capital;

pub use balance_sheet::{
    balance_sheet_ratios, generate_totals, BalanceSheetInput, BalanceSheetRatios,
    BalanceSheetTotals,
};
pub use comparative::{
    generate_periods, horizontal_analysis, horizontal_items, trend_analysis, variance_analysis,
    HorizontalItem, PeriodData, TrendItem, TrendPoint, VarianceLine, VarianceReport,
};
pub use income_statement::{prepare_income_statement, IncomeStatementInput, IncomeStatementOutput};
pub use management_accounts::{prepare_management_accounts, ManagementAccounts};
pub use management_reporting::{
    generate_management_report, ManagementReport, PerformanceSummary, ReportKpis,
};
pub use profit_loss::{compare_pnl, prepare_pnl, PnLComparison, PnLInput, PnLStatement};
pub use ratios::{calculate_ratios, BalanceSheet, IncomeStatement, RatioInput, RatioOutput};
pub use statements::{prepare_statements, StatementsInput, StatementsOutput};
pub use trading_account::{generate_trading_account, TradingAccountInput, TradingAccountResult};
pub use working_capital::{
    analyze_wc_finance, analyze_working_capital, calculate_factoring, optimize_working_capital,
    FactoringInput, FactoringResult, WcAnalysis, WcError, WcFinanceInput, WcFinanceResult,
    WcOptimization, WcOptimizeInput,
};
