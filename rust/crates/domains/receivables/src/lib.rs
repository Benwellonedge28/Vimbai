#![forbid(unsafe_code)]
//! Receivables: the sales ledger and the three services that measure what will not be collected.
//!
//! Four services live here, together because they answer questions about the same debtors:
//!
//! * *sales-ledger-control-service* ([`sales_ledger`]): debtor transactions, the per-debtor balance
//!   replayed from them, the control account and its reconciliation;
//! * *bad-debts-recovery-service* ([`bad_debts`]): debts written off and what has been recovered since;
//! * *provision-bad-debts-service* ([`provision`]): a provision aged from a rule ladder and the journal that
//!   moves it;
//! * *provision-doubtful-debts-service* ([`doubtful`]): a provision graded per debtor, escalated by lateness.
//!
//! Shared arithmetic lives in [`rates`], so the two provisions cannot apply a percentage two different ways,
//! and the calendar-day count comes from `vimbai_common::dates` rather than being written out again here.
//! Every record is held through the store, scoped to the caller's Book.

pub mod bad_debts;
pub mod doubtful;
pub mod provision;
pub mod rates;
pub mod sales_ledger;
