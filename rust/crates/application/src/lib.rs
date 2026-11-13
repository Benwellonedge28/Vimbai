//! The calculation surface a client calls.
//!
//! *realtime-calculation-engine* ([`realtime`]) is this crate's first resident. Its four figures are not its own: a net
//! present value, an internal rate of return and a depreciation schedule are each worked out by the calculator in the
//! workspace that already owns that arithmetic, and a loan paydown is the same paydown the stored loans walk. This layer
//! exists so the application has one door to those figures without ever becoming a second place they are calculated.

#![forbid(unsafe_code)]

pub mod realtime;
