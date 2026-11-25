# Implementation sequencing and acceptance
Status: proposed supplement to ../ROADMAP.md, not replacement of existing phases.

## Dependencies
1. Audit pinned source tree, map existing files and contracts, reconcile documentation discrepancies, approve ADR amendments.
2. Freeze shared contracts: identity/scope, exact money, policy versions, errors, idempotency and provider capabilities.
3. Extend tenancy/access and storage/migration foundations; prove isolation and atomicity.
4. Extend accounting, journal/ledger projections, standards, FX and business documents; prove financial invariants.
5. Implement Book navigation and workspaces using authorized module catalogue; test device integration and offline behavior.
6. Implement commercial catalogue, subscriptions, referrals and partners; external merchant integration only after agreement and policy review.
7. Package Android pilot, then other platforms and self-hosting profiles; verify restore and upgrade.
8. Extend government, AI and interoperability only after relevant controls and expert review.
9. Run production gates, pilot, measure and release incrementally.

## File-level specification gate
Before coding each work package, enumerate actual existing files at a pinned commit; identify owner, public API, inputs/outputs, dependencies, invariants, errors, migration, callers, tests and done criteria. New paths are proposals until duplicate search is complete. Prefer independent contracts and pure functions before consumers. Preserve compatibility; changes to frozen contracts require reviewed decision, not silent downstream rewrites.

## Completion
Each package records source commit, approved design, test evidence, remaining risks and reviewer. No source deletion until parity is proven. No application changes are part of this documentation collection. Full source-file audit remains outstanding; these 30 documents establish architecture direction, not exhaustive implementation specifications.
