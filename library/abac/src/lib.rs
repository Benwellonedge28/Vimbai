//! Attribute-based access control for Vimbai.
//!
//! The policy engine compared a rule's field against request data with a small
//! operator set and swallowed type errors. This crate keeps the decision shape
//! (`evaluations`, `triggered_count`, `blocked`, `allowed`) and the deny-overrides
//! rule, and adds: validated operators at creation, warnings and approval
//! obligations, action scoping, priorities, multi-valued attributes, numeric
//! strings, and a reason attached to every rule that did not fire.
//!
//! Caller-owned, no globals, no clock.

#![forbid(unsafe_code)]

pub mod decision;
pub mod operator;
pub mod rule;
pub mod store;

pub use decision::{evaluate, Decision, EvaluationContext, RuleOutcome};
pub use operator::{compare, Comparison, Operator};
pub use rule::{Effect, Rule, RuleError, RuleSet};
pub use store::AbacStore;

#[cfg(test)]
mod tests;
