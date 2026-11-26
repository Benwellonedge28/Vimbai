# Work package 61 — Reconcile exact inventory valuation for a complete period

Source pinned to [ca0a15a4](https://github.com/Benwellonedge28/Vimbai/commit/ca0a15a467b3263bf2ae5a5342ee30ca2120252e).

## Which calculation callers receive

The `inventory-valuation` entry in `rust/apps/api-server/src/lib.rs` calls `performance_measures::inventory_from_json`, exported from `rust/crates/calculations/performance_measures/src/lib.rs`. In `inventory.rs` in that crate, `calculate` aggregates sales across the input and allocates inventory at period level. It permits an oversold quantity and exposes monetary results as floats after exact internal arithmetic. `ValuationMethod::parse` sends unknown methods to weighted average. The assets crate also exports a float-based `inventory_valuation` helper. This package concerns the published calculator and leaves [physical stock count posting](stock-count-receipts.md) to its own design.

## A versioned period result

Add opt-in `calculation_version: "2"` in `calculate_from_json`. Require a declared inclusive period, an explicit method, one item and one currency per request, integer quantities, exact decimal money and stable opaque line IDs. The opening layer is dated before the period. Refuse a positive opening value with zero opening units, negative quantities or costs, duplicate IDs, invalid dates and sales totals that exceed available units. Use checked integer accumulation for quantities. Define the calculator as periodic. A sale within the window is not validated against an intra-period stock balance. Daily availability belongs to the movement history.

Use FIFO oldest first, LIFO newest first or periodic weighted average. Use date and stable line ID as tie-breakers within purchase layers. For FIFO and LIFO trace the units and amount consumed from each layer. Calculate unrounded proportional allocations with decimal or exact rational intermediates and apply one explicit output rounding policy. Round total available cost and cost of sales once at the requested store-supported scale, then compute displayed ending inventory as their difference. Calculate displayed gross profit from the displayed revenue and cost of sales. Return gross margin null when sales revenue is zero. Every monetary output is a decimal string and identifies the currency and precision.

For an oversold request show “Units sold exceed the opening units and purchases for this period.” Name unreadable input IDs and the field for invalid values. For an unknown method say “Choose FIFO, LIFO or weighted average.” A rejected calculation has no output totals.

## Integrate and prove

Keep the old calculator registration and the legacy numeric response for unversioned callers under ADR-004. Route the versioned branch to the authoritative calculation crate. Audit references to the assets crate's float helper before removing it, keeping any historical caller on its existing contract until tested. The calculator records no data, so there is no store migration. Saved inventory or journal values from other features are untouched.

- In `rust/crates/calculations/performance_measures/src/tests.rs`, add `periodic_inventory_reconciles_final_rounding_and_layer_order`. Compare opening layer behavior and equal-date purchases across all methods.
- Add `periodic_inventory_refuses_oversold_unknown_method_and_quantity_overflow` beside it.
- Test the `calculators` registration in `rust/apps/api-server/src/lib.rs` via `inventory_valuation_keeps_legacy_numbers_and_versioned_exact_strings`, including an amount outside the exact integer range of a binary float.

Accept when the returned units and costs reconcile for every accepted request and the same canonical layers yield the same result in any input order.
