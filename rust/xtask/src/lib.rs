//! Repository tooling that outlived the Python scripts it replaced.
//!
//! The binary and the tests call the same functions, so what a gate checks is what the generator writes: a generator
//! tested through a shell invocation is a generator whose output nobody asserts.
#![forbid(unsafe_code)]

pub mod boundaries;
pub mod catalog;
pub mod crates;
pub mod domains;
pub mod flutter;
pub mod gateway;
pub mod manifests;
pub mod openapi;
pub mod reachability;
pub mod servers;
pub mod wiring;
pub mod workflows;
