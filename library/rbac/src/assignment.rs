//! Who holds which role, in which organisation, until when.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::role::{RoleCatalogue, RoleError};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Assignment {
    pub id: String,
    pub user_id: String,
    pub organization_id: String,
    pub role_id: String,
    pub assigned_at_micros: i64,
    pub assigned_by: String,
    /// Zero means no expiry. A temporary elevation carries one.
    pub expires_at_micros: i64,
}

impl Assignment {
    pub fn is_active(&self, now_micros: i64) -> bool {
        self.expires_at_micros == 0 || self.expires_at_micros > now_micros
    }
}

#[derive(Debug, Clone, Default)]
pub struct AssignmentStore {
    assignments: IndexMap<String, Assignment>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssignmentError {
    NotFound,
    Duplicate,
    UnknownRole,
}

impl AssignmentError {
    pub fn status_code(&self) -> u16 {
        match self {
            AssignmentError::NotFound => 404,
            AssignmentError::Duplicate => 409,
            AssignmentError::UnknownRole => 404,
        }
    }

    pub fn message(&self) -> String {
        match self {
            AssignmentError::NotFound => "Assignment not found".to_string(),
            AssignmentError::Duplicate => {
                "Role already assigned to this user in this org".to_string()
            }
            AssignmentError::UnknownRole => "Role not found".to_string(),
        }
    }
}

impl AssignmentStore {
    pub fn new() -> Self {
        AssignmentStore::default()
    }

    /// Assign a role. The role must exist, and the same role cannot be assigned
    /// twice to one person in one organisation.
    #[allow(clippy::too_many_arguments)] // mirrors the assignment endpoint's fields
    pub fn assign(
        &mut self,
        catalogue: &RoleCatalogue,
        id: &str,
        user_id: &str,
        organization_id: &str,
        role_id: &str,
        assigned_by: &str,
        now_micros: i64,
        expires_at_micros: i64,
    ) -> Result<Assignment, AssignmentError> {
        if catalogue.get(role_id).is_none() {
            return Err(AssignmentError::UnknownRole);
        }
        if self
            .active_duplicate(user_id, organization_id, role_id, now_micros)
            .is_some()
        {
            return Err(AssignmentError::Duplicate);
        }
        let assignment = Assignment {
            id: id.to_string(),
            user_id: user_id.to_string(),
            organization_id: organization_id.to_string(),
            role_id: role_id.to_string(),
            assigned_at_micros: now_micros,
            assigned_by: assigned_by.to_string(),
            expires_at_micros,
        };
        self.assignments.insert(id.to_string(), assignment.clone());
        Ok(assignment)
    }

    fn active_duplicate(
        &self,
        user_id: &str,
        organization_id: &str,
        role_id: &str,
        now_micros: i64,
    ) -> Option<&Assignment> {
        self.assignments.values().find(|assignment| {
            assignment.user_id == user_id
                && assignment.organization_id == organization_id
                && assignment.role_id == role_id
                && assignment.is_active(now_micros)
        })
    }

    pub fn revoke(&mut self, assignment_id: &str) -> Result<Assignment, AssignmentError> {
        self.assignments
            .shift_remove(assignment_id)
            .ok_or(AssignmentError::NotFound)
    }

    pub fn get(&self, assignment_id: &str) -> Option<&Assignment> {
        self.assignments.get(assignment_id)
    }

    /// A person's live roles in one organisation, optionally including expired
    /// ones for an admin view.
    pub fn for_user(
        &self,
        user_id: &str,
        organization_id: Option<&str>,
        now_micros: i64,
        include_expired: bool,
    ) -> Vec<Assignment> {
        self.assignments
            .values()
            .filter(|assignment| assignment.user_id == user_id)
            .filter(|assignment| {
                organization_id.is_none_or(|org| assignment.organization_id == org)
            })
            .filter(|assignment| include_expired || assignment.is_active(now_micros))
            .cloned()
            .collect()
    }

    pub fn for_organization(&self, organization_id: &str) -> Vec<Assignment> {
        self.assignments
            .values()
            .filter(|assignment| assignment.organization_id == organization_id)
            .cloned()
            .collect()
    }

    pub fn revoke_all_for_role(&mut self, role_id: &str) -> usize {
        let victims: Vec<String> = self
            .assignments
            .iter()
            .filter(|(_, assignment)| assignment.role_id == role_id)
            .map(|(id, _)| id.clone())
            .collect();
        for id in &victims {
            self.assignments.shift_remove(id);
        }
        victims.len()
    }

    pub fn prune_expired(&mut self, now_micros: i64) -> usize {
        let victims: Vec<String> = self
            .assignments
            .iter()
            .filter(|(_, assignment)| !assignment.is_active(now_micros))
            .map(|(id, _)| id.clone())
            .collect();
        for id in &victims {
            self.assignments.shift_remove(id);
        }
        victims.len()
    }

    pub fn len(&self) -> usize {
        self.assignments.len()
    }

    pub fn is_empty(&self) -> bool {
        self.assignments.is_empty()
    }
}

/// Deleting a role must not leave assignments dangling. The caller checks this
/// first, then decides whether to reassign or to cascade.
pub fn can_delete_role(
    catalogue: &RoleCatalogue,
    store: &AssignmentStore,
    role_id: &str,
    now_micros: i64,
) -> Result<(), RoleError> {
    let role = catalogue.get(role_id).ok_or(RoleError::NotFound)?;
    let held = store
        .for_organization(&role.organization_id)
        .iter()
        .any(|assignment| assignment.role_id == role_id && assignment.is_active(now_micros));
    if held {
        return Err(RoleError::StillAssigned(role_id.to_string()));
    }
    Ok(())
}
