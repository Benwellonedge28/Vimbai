//! Behaviour pinned from org-authorization-engine, plus the additions.

use serde_json::json;

use crate::assignment::AssignmentError;
use crate::permission::Permission;
use crate::role::{Role, RoleCatalogue, RoleError};
use crate::store::RbacStore;

const NOW: i64 = 1_793_693_906_000_000;

fn store() -> RbacStore {
    let mut store = RbacStore::new();
    store
        .create_role(Role::new(
            "role-admin",
            "org-1",
            "finance-admin",
            &["invoices:*", "reports:read"],
        ))
        .unwrap();
    store
        .create_role(Role::new(
            "role-auditor",
            "org-1",
            "auditor",
            &["reports:read", "journals:read"],
        ))
        .unwrap();
    store
}

#[test]
fn a_permission_is_a_resource_and_an_action() {
    let parsed = Permission::parse("invoices:read").unwrap();
    assert_eq!(parsed.resource, "invoices");
    assert_eq!(parsed.action, "read");
    assert_eq!(parsed.as_string(), "invoices:read");
    assert!(Permission::parse("invoices").is_none());
    assert!(Permission::parse(":read").is_none());
    assert!(Permission::parse("invoices:").is_none());
    assert_eq!(
        Permission::parse("  invoices : read ").unwrap().as_string(),
        "invoices:read"
    );
}

#[test]
fn patterns_cover_one_segment_at_a_time() {
    let read = Permission::new("invoices", "read");
    assert!(Permission::pattern_covers("invoices:read", &read));
    assert!(Permission::pattern_covers("invoices:*", &read));
    assert!(Permission::pattern_covers("*:read", &read));
    assert!(Permission::pattern_covers("*:*", &read));
    assert!(!Permission::pattern_covers("journals:*", &read));
    assert!(!Permission::pattern_covers("invoices:write", &read));
    // a bare wildcard is not a permission, and matches nothing
    assert!(!Permission::pattern_covers("*", &read));
}

#[test]
fn the_original_check_contract_is_preserved() {
    let mut store = store();
    store
        .assign("as-1", "user-1", "org-1", "role-admin", "admin", NOW, 0)
        .unwrap();

    let allowed = store.check("user-1", "org-1", "invoices:write", NOW);
    assert!(allowed.allowed);
    assert_eq!(allowed.role.as_deref(), Some("finance-admin"));
    assert_eq!(allowed.reason, "Permission granted");
    assert_eq!(
        allowed.to_json(&Permission::new("invoices", "write"))["allowed"],
        json!(true)
    );

    let missing = store.check("user-1", "org-1", "journals:write", NOW);
    assert!(!missing.allowed);
    assert_eq!(
        missing.reason,
        "Permission not granted by any assigned role"
    );

    let unassigned = store.check("user-9", "org-1", "invoices:read", NOW);
    assert!(!unassigned.allowed);
    assert_eq!(unassigned.reason, "No role assignments found");
}

#[test]
fn assignments_are_unique_per_person_org_and_role() {
    let mut store = store();
    store
        .assign("as-1", "user-1", "org-1", "role-admin", "admin", NOW, 0)
        .unwrap();
    let duplicate = store
        .assign("as-2", "user-1", "org-1", "role-admin", "admin", NOW, 0)
        .unwrap_err();
    assert_eq!(duplicate, AssignmentError::Duplicate);
    assert_eq!(duplicate.status_code(), 409);
    assert_eq!(
        duplicate.message(),
        "Role already assigned to this user in this org"
    );

    // the same role in another organisation is a different assignment
    store
        .assign("as-3", "user-1", "org-2", "role-admin", "admin", NOW, 0)
        .unwrap();
    // and a different role in the same org is fine too
    store
        .assign("as-4", "user-1", "org-1", "role-auditor", "admin", NOW, 0)
        .unwrap();
    assert_eq!(store.assignments.len(), 3);
}

#[test]
fn an_unknown_role_cannot_be_assigned_and_a_missing_one_404s() {
    let mut store = store();
    let unknown = store
        .assign("as-1", "user-1", "org-1", "role-ghost", "admin", NOW, 0)
        .unwrap_err();
    assert_eq!(unknown, AssignmentError::UnknownRole);
    assert_eq!(unknown.status_code(), 404);
    assert_eq!(unknown.message(), "Role not found");

    let revoked = store.revoke("as-missing").unwrap_err();
    assert_eq!(revoked, AssignmentError::NotFound);
    assert_eq!(revoked.message(), "Assignment not found");
}

#[test]
fn revocation_removes_the_grant_immediately() {
    let mut store = store();
    let assignment = store
        .assign("as-1", "user-1", "org-1", "role-admin", "admin", NOW, 0)
        .unwrap();
    assert!(store.check("user-1", "org-1", "invoices:read", NOW).allowed);
    let revoked = store.revoke(&assignment.id).unwrap();
    assert_eq!(revoked.id, "as-1");
    assert!(!store.check("user-1", "org-1", "invoices:read", NOW).allowed);
    assert!(store.revoke("as-1").is_err(), "the second revoke 404s");
}

#[test]
fn roles_inherit_but_a_cycle_is_refused() {
    let mut store = store();
    store
        .create_role(Role::new(
            "role-lead",
            "org-1",
            "finance-lead",
            &["journals:*"],
        ))
        .unwrap();
    store
        .roles
        .set_parents("role-lead", &["role-admin".to_string()])
        .unwrap();
    store
        .assign("as-1", "user-1", "org-1", "role-lead", "admin", NOW, 0)
        .unwrap();

    let decision = store.check("user-1", "org-1", "invoices:read", NOW);
    assert!(decision.allowed, "inherited from finance-admin");
    assert_eq!(decision.role.as_deref(), Some("finance-admin"));
    assert!(
        store
            .check("user-1", "org-1", "journals:write", NOW)
            .allowed
    );

    // admin may not inherit from its own child
    let cycle = store
        .roles
        .set_parents("role-admin", &["role-lead".to_string()])
        .unwrap_err();
    assert!(matches!(cycle, RoleError::InheritanceCycle(_)));
    assert_eq!(cycle.status_code(), 400);

    // nor may it inherit from itself
    assert!(store
        .roles
        .set_parents("role-admin", &["role-admin".to_string()])
        .is_err());
}

#[test]
fn a_deny_beats_a_grant_even_from_a_broader_role() {
    let mut store = store();
    store
        .create_role(
            Role::new("role-chair", "org-1", "committee-chair", &[])
                .inheriting("role-admin")
                .denying(&["invoices:delete"]),
        )
        .unwrap();
    store
        .assign("as-1", "user-1", "org-1", "role-chair", "admin", NOW, 0)
        .unwrap();

    assert!(store.check("user-1", "org-1", "invoices:read", NOW).allowed);
    let denied = store.check("user-1", "org-1", "invoices:delete", NOW);
    assert!(!denied.allowed);
    assert_eq!(denied.reason, "Permission explicitly denied");
    assert!(denied.roles.contains(&"committee-chair".to_string()));
}

#[test]
fn two_roles_add_up() {
    let mut store = store();
    store
        .assign("as-1", "user-1", "org-1", "role-admin", "admin", NOW, 0)
        .unwrap();
    store
        .assign("as-2", "user-1", "org-1", "role-auditor", "admin", NOW, 0)
        .unwrap();
    let effective = store.effective_permissions("user-1", "org-1", NOW);
    assert_eq!(effective.roles, vec!["auditor", "finance-admin"]);
    assert!(effective.listing().contains(&"journals:read".to_string()));
    assert!(effective.allows(&Permission::new("reports", "read")));
}

#[test]
fn a_temporary_elevation_expires_without_anyone_remembering_to_remove_it() {
    let mut store = store();
    store
        .assign(
            "as-1",
            "user-1",
            "org-1",
            "role-admin",
            "admin",
            NOW,
            NOW + 3_600 * 1_000_000,
        )
        .unwrap();
    assert!(store.check("user-1", "org-1", "invoices:read", NOW).allowed);

    let after = NOW + 3_601 * 1_000_000;
    let decision = store.check("user-1", "org-1", "invoices:read", after);
    assert!(!decision.allowed);
    assert_eq!(decision.reason, "No role assignments found");

    // expired assignments can be listed for an admin, then pruned
    assert_eq!(
        store
            .roles_for_user("user-1", Some("org-1"), after, true)
            .len(),
        1
    );
    assert_eq!(
        store
            .roles_for_user("user-1", Some("org-1"), after, false)
            .len(),
        0
    );
    assert_eq!(store.assignments.prune_expired(after), 1);
}

#[test]
fn role_names_are_unique_within_a_deployment_and_permissions_validated() {
    let mut store = store();
    let duplicate = store
        .create_role(Role::new("role-2", "org-1", "Finance-Admin", &[]))
        .unwrap_err();
    assert_eq!(duplicate.status_code(), 409);

    let bad_permission = store
        .create_role(Role::new("role-3", "org-1", "broken", &["invoices"]))
        .unwrap_err();
    assert_eq!(
        bad_permission,
        RoleError::InvalidPermission("invoices".to_string())
    );

    let empty_name = store
        .create_role(Role::new("role-4", "org-1", "   ", &[]))
        .unwrap_err();
    assert_eq!(empty_name, RoleError::InvalidName);
}

#[test]
fn a_role_still_held_cannot_be_deleted() {
    let mut store = store();
    store
        .assign("as-1", "user-1", "org-1", "role-admin", "admin", NOW, 0)
        .unwrap();
    let blocked = store.delete_role("role-admin", NOW).unwrap_err();
    assert_eq!(blocked, RoleError::StillAssigned("role-admin".to_string()));
    assert_eq!(blocked.status_code(), 409);

    store.assignments.revoke("as-1").unwrap();
    assert!(store.delete_role("role-admin", NOW).unwrap());
    assert!(
        store.delete_role("role-admin", NOW).is_err(),
        "already gone"
    );
}

#[test]
fn catalogue_is_scoped_per_organisation() {
    let mut catalogue = RoleCatalogue::new();
    catalogue
        .insert(Role::new("r1", "org-1", "admin", &["*:*"]))
        .unwrap();
    catalogue
        .insert(Role::new("r2", "org-2", "admin", &["invoices:read"]))
        .unwrap();
    assert_eq!(catalogue.all("org-1").len(), 1);
    assert_eq!(catalogue.all("org-2").len(), 1);
    assert!(catalogue.by_name("org-1", "ADMIN").is_some());
    assert!(catalogue.by_name("org-3", "admin").is_none());
    assert_eq!(catalogue.lineage("r1").len(), 1);
}

#[test]
fn health_reports_what_it_holds() {
    let mut store = store();
    store
        .assign("as-1", "user-1", "org-1", "role-admin", "admin", NOW, 0)
        .unwrap();
    let health = store.health();
    assert_eq!(health["status"], "healthy");
    assert_eq!(health["roles"], 2);
    assert_eq!(health["assignments"], 1);
}

#[test]
fn wildcard_roles_list_their_grants_as_written() {
    let mut store = store();
    store
        .assign("as-1", "user-1", "org-1", "role-admin", "admin", NOW, 0)
        .unwrap();
    let effective = store.effective_permissions("user-1", "org-1", NOW);
    assert_eq!(
        effective.listing(),
        vec!["invoices:*".to_string(), "reports:read".to_string()]
    );
    assert!(effective.denied_listing().is_empty());
}
