//! Accounting services: the cash flow statement and the trading account. Statements that read recorded figures and compute the ones they publish.
//!
//! Services here:
//! * `cash-flow-statement-service`
//! * `trading-account-service`
//!
//! One crate per business domain, not one per service: these services were separate Python
//! containers that each carried a copy of the same models, the same database access and the same
//! arithmetic. Grouped, a domain shares one build, one test run and one place to look.

#![forbid(unsafe_code)]

pub mod cash_flow;
pub mod trading_account;
