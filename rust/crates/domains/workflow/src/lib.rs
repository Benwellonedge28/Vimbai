//! *distributed-workflow-orchestration-service* ([`orchestration`]): the multi-step workflows a Book runs and
//! the rollback of what ran, with the shipped limit stated rather than papered over: the steps are recorded and no
//! request is sent to the addresses they name.
//!
#![forbid(unsafe_code)]
//! Workflow service.
//!
//! * *workflow-service* ([`workflow`]): the definitions a step runs through and the instances that walk them.
//!
//! The service keeps its records on the store, scoped to the caller and the Book, and is safe Rust by construction.

pub mod orchestration;
pub mod workflow;
