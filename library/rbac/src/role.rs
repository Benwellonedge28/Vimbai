//! Roles, their inheritance, and the effective permission set they resolve to.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::permission::Permission;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Role {
    pub id: String,
    pub organization_id: String,
    pub name: String,
    pub description: String,
    /// Patterns this role grants.
    pub permissions: Vec<String>,
    /// Patterns this role denies, overriding anything granted.
    pub denies: Vec<String>,
    /// Roles whose grants this role also holds.
    pub inherits: Vec<String>,
    pub created_at_micros: i64,
    pub builtin: bool,
}

impl Role {
    pub fn new(id: &str, organization_id: &str, name: &str, permissions: &[&str]) -> Role {
        Role {
            id: id.to_string(),
            organization_id: organization_id.to_string(),
            name: name.to_string(),
            description: String::new(),
            permissions: permissions.iter().map(|p| p.to_string()).collect(),
            denies: Vec::new(),
            inherits: Vec::new(),
            created_at_micros: 0,
            builtin: false,
        }
    }

    pub fn with_description(mut self, description: &str) -> Role {
        self.description = description.to_string();
        self
    }

    pub fn inheriting(mut self, role_id: &str) -> Role {
        self.inherits.push(role_id.to_string());
        self
    }

    pub fn denying(mut self, permissions: &[&str]) -> Role {
        self.denies
            .extend(permissions.iter().map(|p| p.to_string()));
        self
    }
}

/// Roles held by one organisation. Caller-owned, as everything in Vimbai is.
#[derive(Debug, Clone, Default)]
pub struct RoleCatalogue {
    roles: IndexMap<String, Role>,
}

impl RoleCatalogue {
    pub fn new() -> Self {
        RoleCatalogue::default()
    }

    pub fn insert(&mut self, role: Role) -> Result<(), RoleError> {
        if role.name.trim().is_empty() {
            return Err(RoleError::InvalidName);
        }
        if self.roles.values().any(|existing| {
            existing.id != role.id
                && existing.organization_id == role.organization_id
                && existing.name.eq_ignore_ascii_case(&role.name)
        }) {
            return Err(RoleError::DuplicateName(role.name.clone()));
        }
        for permission in role.permissions.iter().chain(role.denies.iter()) {
            if Permission::parse(permission.trim_start_matches('!')).is_none() {
                return Err(RoleError::InvalidPermission(permission.clone()));
            }
        }
        self.roles.insert(role.id.clone(), role);
        Ok(())
    }

    /// Persist a hierarchy edge, refusing one that would close a cycle.
    pub fn set_parents(&mut self, role_id: &str, parents: &[String]) -> Result<(), RoleError> {
        if !self.roles.contains_key(role_id) {
            return Err(RoleError::NotFound);
        }
        for parent in parents {
            if parent == role_id || self.reaches(parent, role_id) {
                return Err(RoleError::InheritanceCycle(role_id.to_string()));
            }
            if !self.roles.contains_key(parent) {
                return Err(RoleError::NotFound);
            }
        }
        if let Some(role) = self.roles.get_mut(role_id) {
            role.inherits = parents.to_vec();
        }
        Ok(())
    }

    fn reaches(&self, from: &str, target: &str) -> bool {
        let mut seen = Vec::new();
        let mut queue = vec![from.to_string()];
        while let Some(current) = queue.pop() {
            if current == target {
                return true;
            }
            if seen.contains(&current) {
                continue;
            }
            seen.push(current.clone());
            if let Some(role) = self.roles.get(&current) {
                queue.extend(role.inherits.iter().cloned());
            }
        }
        false
    }

    pub fn get(&self, role_id: &str) -> Option<&Role> {
        self.roles.get(role_id)
    }

    pub fn by_name(&self, organization_id: &str, name: &str) -> Option<&Role> {
        self.roles.values().find(|role| {
            role.organization_id == organization_id && role.name.eq_ignore_ascii_case(name)
        })
    }

    pub fn all(&self, organization_id: &str) -> Vec<Role> {
        self.roles
            .values()
            .filter(|role| role.organization_id == organization_id)
            .cloned()
            .collect()
    }

    pub fn remove(&mut self, role_id: &str) -> bool {
        self.roles.shift_remove(role_id).is_some()
    }

    pub fn len(&self) -> usize {
        self.roles.len()
    }

    pub fn is_empty(&self) -> bool {
        self.roles.is_empty()
    }

    /// Every role reachable from `role_id`, including itself (nearest first).
    pub fn lineage(&self, role_id: &str) -> Vec<Role> {
        let mut lineage = Vec::new();
        let mut seen = Vec::new();
        let mut queue = vec![role_id.to_string()];
        while let Some(current) = queue.pop() {
            if seen.contains(&current) {
                continue;
            }
            if let Some(role) = self.roles.get(&current) {
                lineage.push(role.clone());
                queue.extend(role.inherits.iter().cloned());
            }
            seen.push(current);
        }
        lineage
    }

    /// Resolve what a set of roles actually grants.
    ///
    /// Denies win, and a deny in a nearer role beats a grant in a farther one,
    /// so a specific role can withhold an action from a broad parent.
    pub fn effective_permissions(&self, role_ids: &[String]) -> EffectivePermissions {
        let mut granted: Vec<(Vec<String>, Permission)> = Vec::new();
        let mut denied: Vec<(Vec<String>, Permission)> = Vec::new();
        let mut names = Vec::new();

        for role_id in role_ids {
            let lineage = self.lineage(role_id);
            for (depth, role) in lineage.iter().enumerate() {
                names.push(role.name.clone());
                for pattern in &role.permissions {
                    let parsed = Permission::parse(pattern).expect("validated on insert");
                    granted.push((vec![format!("#{depth}:{}", role.id)], parsed));
                }
                for pattern in &role.denies {
                    let parsed = Permission::parse(pattern).expect("validated on insert");
                    denied.push((vec![format!("#{depth}:{}", role.id)], parsed));
                }
            }
        }

        names.sort();
        names.dedup();
        EffectivePermissions {
            roles: names,
            granted,
            denied,
        }
    }
}

/// The permissions a user holds, with the evidence for each decision.
#[derive(Debug, Clone, PartialEq)]
pub struct EffectivePermissions {
    pub roles: Vec<String>,
    granted: Vec<(Vec<String>, Permission)>,
    denied: Vec<(Vec<String>, Permission)>,
}

impl EffectivePermissions {
    /// Is the permission granted, ignoring denies? Used by the explanation path.
    pub fn is_granted(&self, requested: &Permission) -> bool {
        self.granted
            .iter()
            .any(|(_, pattern)| Permission::pattern_covers(&pattern.as_string(), requested))
    }

    pub fn is_denied(&self, requested: &Permission) -> bool {
        self.denied
            .iter()
            .any(|(_, pattern)| Permission::pattern_covers(&pattern.as_string(), requested))
    }

    pub fn allows(&self, requested: &Permission) -> bool {
        self.is_granted(requested) && !self.is_denied(requested)
    }

    /// All permissions the roles cover, as `resource:action` strings, with
    /// wildcards left as they are so a UI can explain the broad grants.
    pub fn listing(&self) -> Vec<String> {
        let mut listing: Vec<String> = self
            .granted
            .iter()
            .map(|(_, permission)| permission.as_string())
            .filter(|permission| !self.is_denied(&Permission::parse(permission).unwrap()))
            .collect();
        listing.sort();
        listing.dedup();
        listing
    }

    pub fn denied_listing(&self) -> Vec<String> {
        let mut listing: Vec<String> = self
            .denied
            .iter()
            .map(|(_, permission)| permission.as_string())
            .collect();
        listing.sort();
        listing.dedup();
        listing
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoleError {
    NotFound,
    DuplicateName(String),
    InvalidName,
    InvalidPermission(String),
    InheritanceCycle(String),
    StillAssigned(String),
}

impl RoleError {
    pub fn status_code(&self) -> u16 {
        match self {
            RoleError::NotFound => 404,
            RoleError::DuplicateName(_) | RoleError::StillAssigned(_) => 409,
            _ => 400,
        }
    }

    pub fn message(&self) -> String {
        match self {
            RoleError::NotFound => "Role not found".to_string(),
            RoleError::DuplicateName(name) => format!("Role already exists: {name}"),
            RoleError::InvalidName => "Role name is required".to_string(),
            RoleError::InvalidPermission(pattern) => {
                format!("Invalid permission pattern: {pattern}")
            }
            RoleError::InheritanceCycle(role_id) => {
                format!("Role inheritance would form a cycle at {role_id}")
            }
            RoleError::StillAssigned(role_id) => {
                format!("Role is still assigned: {role_id}")
            }
        }
    }
}
