#![forbid(unsafe_code)]
//! Vimbai's trusted ledger kernel.
//!
//! 100% safe Rust: no `unsafe` blocks anywhere in this crate. That is enforced by the compiler through
//! `#![forbid(unsafe_code)]` below, not by a grep in CI as this note used to claim - the attribute was missing, so
//! the promise rested on nothing. Every other crate in the workspace carries it too.
//!
//! Three guarantees, matching Vimbai's standing ledger rules:
//! 1. `double_entry` - every journal entry balances, every line is a real
//!    single-sided movement.
//! 2. `hash_chain` - every entry is hash-stamped over its predecessor's
//!    stamp, making the ledger tamper-evident. Entries are immutable;
//!    corrections happen via reversing entries only.
//! 3. `reversal` - a reversing entry must be the exact mirror of the
//!    entry it reverses, and each entry carries at most one reversal.

pub mod analytics;
pub mod double_entry;
pub mod hash_chain;
pub mod reversal;

pub use double_entry::{
    validate_lines, validate_money_lines, JournalLine, ValidationError as DoubleEntryError,
    LEDGER_SCALE,
};
pub use hash_chain::{
    chain_state_from_digests, compute_entry_hash, genesis_stamp, ChainReport, ChainState,
    EntryDigest, GENESIS_HASH,
};
pub use reversal::{check_reversal, ReversalCheck};
