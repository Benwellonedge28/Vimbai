# Work package 43 — Commit department allocations as exact, replayable runs

Proposed design against `b8826fac84d19d1792f7f95dc552f32d8f053755`.

## Observed behavior
`rust/crates/domains/accounting/src/departments.rs::run_allocation` sets the pool status before validation, converts proportions through floating point, creates result rows and then deletes rows matching the pool. The deletion query selects by pool rather than run, so it can also select rows just created. The method returns proposed journal entries.

## Allocation and persistence
Accept a run ID, effective window and a reviewed snapshot of the selected departments and basis inputs. Resolve eligible departments in the Book and validate every explicit inclusion and exclusion. Refuse unsupported basis or method with a message naming the setting. For equal allocation use weight one. For revenue use supplied nonnegative Money weights and reject an all-zero total. Use exact decimal arithmetic, round down to the shared ledger scale, and distribute indivisible units by descending remainder with department ID as the tie breaker. Refuse a pool that cannot be represented at that scale. The sum must equal the pool exactly.

Persist result rows identified by run and department, the pool's active run pointer and input evidence in a single store transaction. A matching run replay returns its stored result. Changed inputs under the same ID return “This allocation run already has different inputs.” Keep earlier runs readable. Update `allocation_results` to project the active run by default. Preserve the adapter in `rust/apps/api-server/src/records.rs::run_allocation`. Proposal lines remain advisory until a separate authorised posting validates real account mappings and period.

## Upgrade and proof
Entries with no run identity remain legacy evidence. Never reconstruct deleted runs from a pool's current status.

- `allocation_remainder_follows_stable_department_order` proves exact conservation for an uneven division.
- `allocation_write_failure_keeps_prior_active_run` faults the last write on both providers.
- `allocation_replay_returns_original_rows_after_a_new_run` proves immutable prior evidence and active pointer behavior.
- `unsupported_basis_leaves_pool_unmodified` covers refusal before state changes.

Done when no successful run loses its own allocation rows and both stores produce identical results.
