//! Vimbai's trusted ledger kernel.
//!
//! 100% safe Rust: no `unsafe` blocks anywhere in this crate (enforced by
//! CI via `cargo forbid`-style grep; the crate depends only on audited,
//! safe APIs).
//!
//! Three guarantees, matching Vimbai's standing ledger rules:
//! 1. `double_entry` - every journal entry balances, every line is a real
//!    single-sided movement.
//! 2. `hash_chain` - every entry is hash-stamped over its predecessor's
//!    stamp, making the ledger tamper-evident. Entries are immutable;
//!    corrections happen via reversing entries only.
//! 3. `reversal` - a reversing entry must be the exact mirror of the
//!    entry it reverses, and each entry carries at most one reversal.

pub mod double_entry;
pub mod hash_chain;
pub mod reversal;

#[cfg(feature = "python")]
pub mod python;

pub use double_entry::{validate_lines, JournalLine, ValidationError as DoubleEntryError};
pub use hash_chain::{
    chain_state_from_digests, compute_entry_hash, genesis_stamp, ChainReport, ChainState,
    EntryDigest, GENESIS_HASH,
};
pub use reversal::{check_reversal, ReversalCheck};
