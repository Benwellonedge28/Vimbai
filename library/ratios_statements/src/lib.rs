#![forbid(unsafe_code)]
//! Ratio analysis and statement generation.
//!
//! Ported so far: ratio-analysis-service. Queued: financial-statements,
//! comparative-financial-statements, income-statement, balance-sheet,
//! trading-account, profit-loss-account, management-accounts,
//! working-capital family, consolidation.

pub mod balance_sheet;
pub mod income_statement;
pub mod ratios;
pub mod statements;

pub use balance_sheet::{
    balance_sheet_ratios, generate_totals, BalanceSheetInput, BalanceSheetRatios,
    BalanceSheetTotals,
};
pub use income_statement::{prepare_income_statement, IncomeStatementInput, IncomeStatementOutput};
pub use ratios::{calculate_ratios, BalanceSheet, IncomeStatement, RatioInput, RatioOutput};
pub use statements::{prepare_statements, StatementsInput, StatementsOutput};
