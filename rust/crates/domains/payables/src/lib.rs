#![forbid(unsafe_code)]
//! Payables: the purchases ledger and the expenses claimed against it.
//!
//! Two services live here, together because they answer questions about the same money going out:
//!
//! * *purchases-ledger-control-service* ([`purchases_ledger`]): creditor transactions, the per-creditor
//!   balance replayed from them, the control account and its reconciliation;
//! * *expense-tracking-service* ([`expenses`]): an employee's expense claims, their approval, rejection
//!   and the summary a finance team reads.
//!
//! The two are deliberately separate ledgers: a creditor balance is replayed from its transactions, while an
//! expense is a single record with its own status, so neither service can read or move the other's records.
//! Every record is held through the store, scoped to the caller's Book.

pub mod expenses;
pub mod purchases_ledger;
