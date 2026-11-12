//! Accounting services: the cash flow statement, the trading account and suspense analysis. Statements that read recorded figures and compute the ones they publish, plus the corrections that clear a suspense balance.
//!
//! Services here:
//! * `cash-flow-statement-service`
//! * `revenue-recognition-service` (contracts with their obligations, the allocation by standalone selling
//!   price, and recognition against one obligation at a time)
//! * `suspense-error-service`
//! * `trading-account-service`
//!
//! One crate per business domain, not one per service: these services were separate Python
//! containers that each carried a copy of the same models, the same database access and the same
//! arithmetic. Grouped, a domain shares one build, one test run and one place to look.

#![forbid(unsafe_code)]

pub mod cash_flow;
pub mod revenue_recognition;
pub mod suspense;
pub mod trading_account;
pub use revenue_recognition::{
    allocate_price, create_contract as create_revenue_contract,
    list_contracts as list_revenue_contracts, recognise, recognise_revenue, revenue_summary,
};
