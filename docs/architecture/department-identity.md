# Work package 42 — Preserve department identity and hierarchy

Proposed design against `b8826fac84d19d1792f7f95dc552f32d8f053755`.

## Observed behavior
`rust/crates/domains/accounting/src/departments.rs::update_department` deletes the department and creates a replacement. `department_hierarchy` follows IDs in parent fields. A replacement can strand references to the old ID, and a failed create can leave the department missing.

## Proposed contract
Update the record in place using the original Book and ID. Distinguish omitted fields from explicit clearing. Preserve omitted fields, permit explicit clearing of optional parent and manager references, and refuse clearing required code or name. Validate every proposed parent against the same Book. Walk parent IDs until a root, rejecting self-parenting, cycles and missing ancestors. Serialize validation and mutation against competing parent changes using a Book hierarchy version or store transaction. A conflicting update returns “The department structure changed. Reload it before saving.”

Handle `create_department` and `update_department` through the same parent validator. Have `department_hierarchy` surface incomplete history if it encounters a missing or repeated ancestor. Keep the update and hierarchy adapters in `rust/apps/api-server/src/records.rs` and their existing record identifiers. This operation never writes a ledger entry or reallocates previously attributed amounts.

## Existing Books and acceptance
Existing live IDs require no rewrite. List orphaned children for scoped operator review. Repair a historical parent link only after the operator confirms the intended parent. Do not infer a replacement by department name.

- `department_update_preserves_id_and_children` verifies the route and child lookup after updating a name.
- `concurrent_parent_changes_cannot_create_cycle` races reciprocal reparenting on SQLite and PostgreSQL.
- `department_patch_distinguishes_omitted_and_cleared_parent` covers both forms and a cross-Book parent.
- `legacy_missing_ancestor_returns_incomplete_hierarchy` ensures a broken chain is visible.

Done when a failed update leaves the prior row reachable and an accepted update never changes its ID.
