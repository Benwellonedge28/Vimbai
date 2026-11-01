#![forbid(unsafe_code)]
//! Ratio analysis and statement generation.
//!
//! Ported: ratio-analysis, financial-statements, income-statement,
//! balance-sheet, comparative-financial-statements. Queued: trading
//! account, profit and loss, management accounts, working capital
//! family, consolidation.

pub mod balance_sheet;
pub mod comparative;
pub mod income_statement;
pub mod ratios;
pub mod statements;

pub use balance_sheet::{
    balance_sheet_ratios, generate_totals, BalanceSheetInput, BalanceSheetRatios,
    BalanceSheetTotals,
};
pub use comparative::{
    generate_periods, horizontal_analysis, horizontal_items, trend_analysis, variance_analysis,
    HorizontalItem, PeriodData, TrendItem, TrendPoint, VarianceLine, VarianceReport,
};
pub use income_statement::{prepare_income_statement, IncomeStatementInput, IncomeStatementOutput};
pub use ratios::{calculate_ratios, BalanceSheet, IncomeStatement, RatioInput, RatioOutput};
pub use statements::{prepare_statements, StatementsInput, StatementsOutput};
