//! Role-based access control for Vimbai.
//!
//! The org-authorization service resolved permissions by scanning roles for an
//! exact string match. This crate keeps that contract and adds what production
//! needs: permission patterns, role inheritance with cycle refusal, explicit
//! denies, expiring assignments, and an effective-permission view so a UI can
//! explain itself.
//!
//! Everything is caller-owned: one `RbacStore` per Book, no globals, no clock.
//! Timestamps arrive as microseconds since the epoch.

#![forbid(unsafe_code)]

pub mod assignment;
pub mod permission;
pub mod role;
pub mod store;

pub use assignment::{Assignment, AssignmentError, AssignmentStore};
pub use permission::Permission;
pub use role::{EffectivePermissions, Role, RoleCatalogue, RoleError};
pub use store::{Decision, RbacStore};

#[cfg(test)]
mod tests;
