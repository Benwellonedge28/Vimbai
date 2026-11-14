#![forbid(unsafe_code)]
//! The gateway's rules, for a process that has no gateway in front of it.
//!
//! In the cluster the Go gateway verifies the caller's token, injects `X-User-Id`, refuses a Book the caller is not a
//! member of, and rewrites an address the engine answers under a different name. Nothing does that on a laptop, and
//! nothing does it in front of a sync server either, which is why this is a crate rather than a module inside one binary:
//! the rules are the same rules, so the second caller must not restate them.
//!
//! It deliberately does not reimplement the gateway. There is no routing table here, because the engine already answers
//! every address; no membership query, because the engine already holds the Book; and no fallback identity, because a
//! caller without a token is nobody.

pub mod addresses;
pub mod gateway;

pub use gateway::Gateway;
