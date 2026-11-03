//! Vimbai performance measures.
//!
//! Six stateless services: balanced scorecard, internal controls testing, profit,
//! inventory valuation, net realisable value, and foreign exchange. No database,
//! no clock; a caller supplies the date where a response needs one.

#![forbid(unsafe_code)]

pub mod controls;
pub mod fx;
pub mod inventory;
pub mod money;
pub mod profit;
pub mod realizable;
pub mod scorecard;

pub use controls::{
    test as test_controls, Control, DeficiencyLevel, TestingRequest, TestingResult,
};
pub use fx::{
    convert, hedge, rate_of, ConversionRequest, ConversionResult, HedgeRequest, HedgeResult,
    UnsupportedCurrency, RATES,
};
pub use inventory::{
    calculate as value_inventory, PurchaseRecord, SaleRecord, ValuationMethod, ValuationRequest,
    ValuationResult,
};
pub use money::{money, python_float, round_dp};
pub use profit::{
    average_accounting_profit, average_profit, calculate as calculate_profit, from_cash_flow,
    AverageAccountingProfit, CashFlowProfit, ProfitCalculation,
};
pub use realizable::{
    net_realizable_value, residual_value, scrap_value, NrvResult, ResidualValueResult,
    ScrapValueResult,
};
pub use scorecard::{score, Kpi, ScorecardRequest, ScorecardResult, PERSPECTIVES};

#[cfg(test)]
mod tests;
