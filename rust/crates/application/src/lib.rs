//! The calculation surface a client calls.
//!
//! *realtime-calculation-engine* ([`realtime`]) is this crate's first resident. Its four figures are not its own: a net
//! present value, an internal rate of return and a depreciation schedule are each worked out by the calculator in the
//! workspace that already owns that arithmetic, and a loan paydown is the same paydown the stored loans walk. This layer
//! exists so the application has one door to those figures without ever becoming a second place they are calculated.
//!
//! *calculation-api* lived here too, and it published no endpoints of its own: it was the doorway onto this library, and
//! the figures it fronted are the ones above. It is named here because a service the catalogue calls served must be
//! named by the crate that houses it - and so that its purpose is recorded rather than counted as forgotten.

#![forbid(unsafe_code)]

pub mod realtime;
