//! Repository tooling that outlived the Python scripts it replaced.
//!
//! The binary and the tests call the same functions, so what a gate checks is what the generator writes: a generator
//! tested through a shell invocation is a generator whose output nobody asserts.
#![forbid(unsafe_code)]

pub mod catalog;
pub mod contracts;
pub mod crates;
pub mod gateway;
pub mod manifests;
pub mod reachability;
pub mod wiring;
pub mod workflows;
