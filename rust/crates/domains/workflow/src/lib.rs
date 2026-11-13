#![forbid(unsafe_code)]
//! Workflow service.
//!
//! * *workflow-service* ([`workflow`]): the definitions a step runs through and the instances that walk them.
//!
//! The service keeps its records on the store, scoped to the caller and the Book, and is safe Rust by construction.

pub mod workflow;
