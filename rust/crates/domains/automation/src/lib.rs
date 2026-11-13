//! Vimbai automation engine as a library.
//!
//! The scheduler, its task store and its execution history were a FastAPI
//! service holding module-level globals. Everything here is caller-owned state
//! plus pure transitions, so a Book owns its own schedule and two callers can
//! never see each other's tasks.
//!
//! Timing is supplied by the caller (ids and timestamps), which keeps the
//! transitions testable and means this crate never reads a clock.

//! *automation-engine-service* ([`automations`]): the rules a Book automates, their switches, and the runs
//! behind them, kept on the store and scoped to the caller and the Book.
//!
#![forbid(unsafe_code)]

pub mod automations;
pub mod defaults;
pub mod engine;
pub mod execution;
pub mod task;
pub mod types;

#[cfg(test)]
mod tests;

pub use engine::{Engine, EngineError, Metrics, Trigger};
pub use execution::TaskExecution;
pub use task::{TaskDefinition, TaskRequest};
pub use types::{TaskStatus, TaskType};
