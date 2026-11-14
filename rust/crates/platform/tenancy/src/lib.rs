//! The tenant boundary.
//!
//! A person does not open the ledger directly: they belong to an organisation, the organisation owns Books, and a Book
//! owns the records. Until this crate existed the middle of that sentence was missing - an organisation was a record
//! with a name, membership was a field on the account, and a Book was stamped with whoever created it, so "may this
//! person open this Book" was answered in one place in the cluster (the Go gateway) and not at all in the engine.
//!
//! Two things live here and nothing else does: the organisation and its members ([`organisation`]), and the single rule
//! that decides whether a caller may open a Book ([`access`]). Both binaries ask that rule rather than carrying their
//! own, which is the point: a membership check that exists twice is a membership check that will disagree with itself.
#![forbid(unsafe_code)]

pub mod access;
pub mod organisation;

pub use access::{allows, Access};
pub use organisation::{ORGANIZATION, ORGANIZATION_MEMBERSHIP};
