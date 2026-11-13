#![forbid(unsafe_code)]
//! The ledger kernel's services, together because they all answer questions about the same journal.
//!
//! This crate is the kernel's HTTP and record surface: the four services sit beside `ledger-core`,
//! not in a domain of their own, because they all answer questions about the same journal.
//! `ledger-core` stays the sole authority for posting integrity - balanced entries, immutability,
//! reversal rules and the hash chain - and this crate keeps no second copy of those rules.
//!
//! * *double-entry-principles-service* ([`double_entry`]): the rules of double entry, and the entry that
//!   has to satisfy them. Nothing here is stored.
//! * *control-account-reconciliation-service* ([`control_account`]): the breaks between a control account
//!   and its ledger, and the reports that compare the two figures.
//! * *financial-integrity-service* ([`integrity`]): a balance check, a hash check and a completeness check
//!   over a company's records, and the report they add up to.
//! * *financial-state-machine-service* ([`state_machine`]): the life of a document, draft to archived, with
//!   every move checked against the map and written into the document's history.
//!
//! Three of the four own records, held through the store and scoped to the caller's Book; the fourth is a
//! reference and stores nothing. Money is exact throughout, so the balance and reconciliation answers are
//! decided on the amounts themselves rather than on a float approximation of them.
//!
//! The boundary this crate used to name is now closed: `ledger-core::double_entry::validate_money_lines` decides the
//! rules on the amounts themselves, and `/validate-journal-entry` calls it rather than keeping a second copy of double
//! entry. The kernel reads a caller's `f64` back to the decimal it was written as (and to ten places, so `0.1 + 0.2`
//! reads as `0.3`) and then compares the two sides for equality: there is no money tolerance anywhere in this crate or
//! in the kernel, and a cent out of balance is refused.

pub mod control_account;
pub mod double_entry;
pub mod integrity;
pub mod state_machine;
