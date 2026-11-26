# Work package 88 — Retain the weights behind a cost apportionment

Basis-apportionment baseline [5c2d0018](https://github.com/Benwellonedge28/Vimbai/commit/5c2d0018ed10d212ab43eda94a0a8d2509c3ea0d).

## Current allocation inputs

`apportion` in `rust/crates/domains/costing/src/basis_apportionment.rs` falls back to zero when a centre's basis amount cannot be read. It sends positive aggregate weights to `Money::allocate` for largest-remainder splitting. The saved result carries allocations but the all-zero path has no allocation rows. The existing result must distinguish true zero measurements from missing data.

## Reproducible split

For a verified run record Book, effective period, cost currency, exact total, basis unit, policy revision, and unique stable centre IDs and weights. Resolve centres through the Book's configured costing dimension. Require decimal weights at or above zero, and preserve zero explicitly. Refuse missing, unreadable and negative weights by centre ID. Sort by stable centre ID before allocation, so equal residuals resolve deterministically. Use the existing largest-remainder allocator at the chosen supported currency precision. Require the exact input total to fit that precision before splitting.

If all weights are zero, record each zero input, zero per-centre allocations and the entire amount as unapportioned. For a positive weight sum, each part must be nonnegative and parts must add to the requested amount exactly. Save the frozen inputs with the output under `APPORTIONMENT_RESULT`. A repeated operation key returns the same result if the canonical inputs agree. The worksheet does not post overhead. A later adjustment changes the calculation in a new revision and has no effect on the old run.

## API and legacy data

Have the apportionment handlers in `rust/apps/api-server/src/records.rs` preserve existing basis-specific routes and carry an explicit evidence version. Return the totals and stable run ID in the opt-in response. Use `results` to expose each scoped run's evidence. Historical runs lacking submitted weights remain readable as legacy. A stored zero cannot establish whether an input was unreadable.

For absent or negative weights show “Provide a nonnegative basis for this cost centre.” For duplicate centres say “Use each cost centre once in the allocation.”

## Verification

- In the module tests add `apportionment_conserves_currency_and_breaks_equal_remainders_by_id` and `all_zero_basis_preserves_inputs_and_leaves_full_total_unallocated`.
- Add `apportionment_run_rejects_unreadable_weight_before_save` to `rust/apps/api-server/tests/costing_prefixes_http.rs`, checking duplicate and foreign centre IDs separately.

Accept when the same inputs in a different order give the same allocation and retained run data explains every minor unit.
