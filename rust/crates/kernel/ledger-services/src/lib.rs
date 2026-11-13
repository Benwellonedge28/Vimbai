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
//! One boundary is named rather than crossed quietly: `ledger-core::double_entry::validate_lines` decides
//! balance in `f64` with a 1e-9 tolerance, which would put floats back into a posting decision. This crate
//! answers exactly, in `Money`, and therefore does not call it. Moving the kernel's validator onto `Money`
//! is a change to `ledger-core` and belongs in that crate, not here.

pub mod control_account;
pub mod double_entry;
pub mod integrity;
pub mod state_machine;
