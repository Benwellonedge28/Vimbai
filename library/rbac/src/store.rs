//! The facade: create roles, assign them, and answer "may this person do this?"

use serde_json::{json, Value};

use crate::assignment::{can_delete_role, Assignment, AssignmentError, AssignmentStore};
use crate::permission::Permission;
use crate::role::{EffectivePermissions, Role, RoleCatalogue, RoleError};

#[derive(Debug, Clone, PartialEq)]
pub struct Decision {
    pub allowed: bool,
    /// The role that granted it, when one did.
    pub role: Option<String>,
    pub reason: String,
    pub roles: Vec<String>,
}

impl Decision {
    pub fn to_json(&self, permission: &Permission) -> Value {
        json!({
            "allowed": self.allowed,
            "permission": permission.as_string(),
            "role": self.role,
            "reason": self.reason,
            "roles": self.roles,
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct RbacStore {
    pub roles: RoleCatalogue,
    pub assignments: AssignmentStore,
}

impl RbacStore {
    pub fn new() -> Self {
        RbacStore::default()
    }

    pub fn create_role(&mut self, role: Role) -> Result<Role, RoleError> {
        self.roles.insert(role.clone())?;
        Ok(role)
    }

    pub fn list_roles(&self, organization_id: &str) -> Vec<Role> {
        self.roles.all(organization_id)
    }

    pub fn delete_role(&mut self, role_id: &str, now_micros: i64) -> Result<bool, RoleError> {
        can_delete_role(&self.roles, &self.assignments, role_id, now_micros)?;
        Ok(self.roles.remove(role_id))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn assign(
        &mut self,
        id: &str,
        user_id: &str,
        organization_id: &str,
        role_id: &str,
        assigned_by: &str,
        now_micros: i64,
        expires_at_micros: i64,
    ) -> Result<Assignment, AssignmentError> {
        self.assignments.assign(
            &self.roles,
            id,
            user_id,
            organization_id,
            role_id,
            assigned_by,
            now_micros,
            expires_at_micros,
        )
    }

    pub fn revoke(&mut self, assignment_id: &str) -> Result<Assignment, AssignmentError> {
        self.assignments.revoke(assignment_id)
    }

    pub fn roles_for_user(
        &self,
        user_id: &str,
        organization_id: Option<&str>,
        now_micros: i64,
        include_expired: bool,
    ) -> Vec<Assignment> {
        self.assignments
            .for_user(user_id, organization_id, now_micros, include_expired)
    }

    /// The permissions a person holds right now, with the roles that produced
    /// them. This is what a UI needs to grey out a button correctly.
    pub fn effective_permissions(
        &self,
        user_id: &str,
        organization_id: &str,
        now_micros: i64,
    ) -> EffectivePermissions {
        let role_ids: Vec<String> = self
            .assignments
            .for_user(user_id, Some(organization_id), now_micros, false)
            .iter()
            .map(|assignment| assignment.role_id.clone())
            .collect();
        self.roles.effective_permissions(&role_ids)
    }

    /// The authorisation question. Wording and shape match the service that
    /// preceded this crate, so a client cannot tell which is answering.
    pub fn check(
        &self,
        user_id: &str,
        organization_id: &str,
        permission_text: &str,
        now_micros: i64,
    ) -> Decision {
        let Some(permission) = Permission::parse(permission_text) else {
            return Decision {
                allowed: false,
                role: None,
                reason: format!("Malformed permission: {permission_text}"),
                roles: Vec::new(),
            };
        };
        let assignments =
            self.assignments
                .for_user(user_id, Some(organization_id), now_micros, false);
        if assignments.is_empty() {
            return Decision {
                allowed: false,
                role: None,
                reason: "No role assignments found".to_string(),
                roles: Vec::new(),
            };
        }
        let role_ids: Vec<String> = assignments
            .iter()
            .map(|assignment| assignment.role_id.clone())
            .collect();
        let effective = self.roles.effective_permissions(&role_ids);

        if effective.is_denied(&permission) {
            return Decision {
                allowed: false,
                role: None,
                reason: "Permission explicitly denied".to_string(),
                roles: effective.roles.clone(),
            };
        }
        // Name the nearest role that grants it, which is the most specific.
        for role_id in &role_ids {
            for role in self.roles.lineage(role_id) {
                if role
                    .permissions
                    .iter()
                    .any(|pattern| Permission::pattern_covers(pattern, &permission))
                {
                    return Decision {
                        allowed: true,
                        role: Some(role.name.clone()),
                        reason: "Permission granted".to_string(),
                        roles: effective.roles.clone(),
                    };
                }
            }
        }
        Decision {
            allowed: false,
            role: None,
            reason: "Permission not granted by any assigned role".to_string(),
            roles: effective.roles.clone(),
        }
    }

    pub fn health(&self) -> Value {
        json!({
            "status": "healthy",
            "service": "org-authorization",
            "version": "2.0.0",
            "roles": self.roles.len(),
            "assignments": self.assignments.len(),
        })
    }
}
