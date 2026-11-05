#![forbid(unsafe_code)]
//! Ratio analysis and statement generation.
//!
//! Ported: ratio-analysis, financial-statements, income-statement,
//! balance-sheet, comparative-financial-statements, trading account,
//! profit and loss, management accounts and reporting, working capital
//! family. Queued: consolidation.

pub mod balance_sheet;
pub mod basis_apportionment;
pub mod comparative;
pub mod consolidation;
pub mod income_statement;
pub mod interim_reporting;
pub mod investor_metrics;
pub mod ledger_controls;
pub mod management_accounts;
pub mod profit_loss;
pub mod ratios;
pub mod segment_reporting;
pub mod statements;
pub mod trading_account;
pub mod working_capital;

pub use balance_sheet::{
    balance_sheet_ratios, generate_totals, BalanceSheetInput, BalanceSheetRatios,
    BalanceSheetTotals,
};
pub use basis_apportionment::{
    apportion, ApportionedCentre, ApportionmentBasis, ApportionmentResult, CostCentreBasis,
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
pub use profit_loss::{compare_pnl, prepare_pnl, PnLComparison, PnLInput, PnLStatement};
pub use ratios::{calculate_ratios, BalanceSheet, IncomeStatement, RatioInput, RatioOutput};
pub use statements::{prepare_statements, StatementsInput, StatementsOutput};
pub use trading_account::{generate_trading_account, TradingAccountInput, TradingAccountResult};
pub use working_capital::{
    analyze_wc_finance, analyze_working_capital, calculate_factoring, optimize_working_capital,
    FactoringInput, FactoringResult, WcAnalysis, WcError, WcFinanceInput, WcFinanceResult,
    WcOptimization, WcOptimizeInput,
};
