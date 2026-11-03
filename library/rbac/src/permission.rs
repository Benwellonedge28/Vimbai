//! Permission strings and the patterns that grant them.
//!
//! A permission is `resource:action`, and a role holds patterns:
//! `invoices:read`, `invoices:*`, `*:read`, `*:*`. A leading `!` in a role's deny
//! list removes a permission an earlier role granted, because "chair of finance"
//! should be able to withhold one action from "finance admin" without inventing
//! a parallel role.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Permission {
    pub resource: String,
    pub action: String,
}

impl Permission {
    pub fn new(resource: &str, action: &str) -> Permission {
        Permission {
            resource: resource.to_string(),
            action: action.to_string(),
        }
    }

    /// Parse `invoices:read`. A missing colon is a bad request, not a wildcard.
    pub fn parse(text: &str) -> Option<Permission> {
        let (resource, action) = text.split_once(':')?;
        let resource = resource.trim();
        let action = action.trim();
        if resource.is_empty() || action.is_empty() {
            return None;
        }
        Some(Permission::new(resource, action))
    }

    pub fn as_string(&self) -> String {
        format!("{}:{}", self.resource, self.action)
    }

    /// Does this pattern cover the requested permission? `*` matches one
    /// segment; a pattern is otherwise exact.
    pub fn pattern_covers(pattern: &str, requested: &Permission) -> bool {
        let Some(parsed) = Permission::parse(pattern) else {
            return false;
        };
        let resource_ok = parsed.resource == "*" || parsed.resource == requested.resource;
        let action_ok = parsed.action == "*" || parsed.action == requested.action;
        resource_ok && action_ok
    }
}
