#![forbid(unsafe_code)]
//! Banking: what the bank says happened, what the books say happened, and the reconciliation of the two.
//!
//! One crate for the whole banking domain, because these five services are five views of the same subject - a bank
//! account, the money moving through it, and the people who run it:
//!
//! * *bank-reconciliation-service* ([`reconciliation`]): statement lines imported from a bank, cash book entries
//!   taken from the books, the automatic matching of one against the other, the adjustment journal, and the list
//!   of what is still outstanding;
//! * *currency-service* ([`currency`]): the currencies a Book deals in, the rate series between them, and the
//!   conversions over those rates;
//! * *bank-feed-service* ([`bank_feed`]): the connections a Book holds, the transactions that arrive over them,
//!   the syncs, the rules that pick transactions out, MT940 parsing and provider webhooks;
//! * *bank-relationship-service* ([`relationship`]): the banks a company deals with, the quality of their service,
//!   and the summary of the whole relationship book;
//! * *banking-integration-service* ([`integration`]): the plain connection door, the sync over it, and the
//!   reconciliation of what arrived.
//!
//! Every record is held through the store, scoped to the caller's Book. Two record kinds are shared deliberately
//! because two services described the same thing: the bank connection (one `BankConnection` on one edge, whichever
//! door opened it) and the imported bank transaction (one set of rows, whichever door read it). Reconciliation
//! records, by contrast, are held under their own label: the shipped service shared `BankReconciliation` with
//! cashbook-service, which stores a different shape there, so the two could read each other's rows.

pub mod bank_feed;
pub mod currency;
pub mod integration;
pub mod reconciliation;
pub mod relationship;
