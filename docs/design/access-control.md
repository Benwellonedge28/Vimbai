# Access control
Status: proposed extension of ADR-010 and ../domains/organisations.md. Existing membership gate is coarse; granular permissions are not assumed implemented.

## Decision model
Authenticate principal, resolve active tenant and Book relationship (ReBAC), expand scoped role grants (RBAC), evaluate attributes such as period, amount, ownership, approval state and device trust (ABAC), then apply explicit denies and mandatory separation-of-duty constraints. Default deny; no entitlement overrides security. Inherit organisation grants into its Books only when scope permits; direct Book membership remains supported. No inheritance across tenants. Revocation invalidates cached decisions; offline high-risk actions require fresh authorization or remain pending.

## Proposed permission matrix
| Role | Read | Draft | Post | Approve | Manage grants |
|---|---|---|---|---|---|
| Viewer | yes | no | no | no | no |
| Clerk | scoped | yes | no | no | no |
| Accountant | scoped | yes | policy | no | no |
| Approver | scoped | no | no | policy | no |
| Admin | scoped | policy | policy | policy | yes |
| Auditor | scoped | no | no | no | no |
Owner retains governance but cannot bypass mandatory financial controls. No self-approval where separation required.

## Integration
Extend rust/crates/platform/tenancy/src/access.rs and existing access-control owner; proposed policy evaluator, grant registry and decision audit modules live beside existing authority after inventory. API tenancy_gate checks resource actions; gateway consumes same decisions; local runtime shares evaluator. Flutter receives filtered capability descriptors. Version policies, log decision reason without sensitive attributes.

## Migration and tests
Map legacy owner/admin/member grants explicitly; shadow-evaluate and compare before enforcement; never silently broaden rights. Test cross-tenant IDs, deny precedence, stale grants, role changes, last owner, direct membership, delegation expiry, offline replay and bypass attempts.
