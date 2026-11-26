# Work package 87 — Value closing stock from a stated product-cost revision

Absorption-costing source [5c2d0018](https://github.com/Benwellonedge28/Vimbai/commit/5c2d0018ed10d212ab43eda94a0a8d2509c3ea0d).

## Current selection

`latest_product_cost` in `rust/crates/domains/costing/src/absorption.rs` selects the first product match in a capped Book scan. `stock_valuation` always uses that latest unit cost and echoes the caller's method name. `record_product_cost` accepts the submitted unit cost as a stored value. A date or method label alone cannot establish the calculation basis.

## Record and select the valuation basis

A verified cost revision records stable product ID, effective period, currency, quantity unit, exact direct and overhead inputs, units produced, authenticated preparer and source references. For calculated revisions, reuse `calculate_product_cost`. Compute unit cost from exact total cost divided by positive production units. Zero production yields an unavailable per-unit cost. Allow externally reviewed costs only with an explicit separate source type and evidence reference. Maintain prior cost revisions on correction.

A versioned valuation accepts product ID, closing quantity in the same unit, and either an exact cost revision ID or the effective date and selected approved basis. Scope before selection and never choose a future revision. If more than one revision is applicable after supersession, ask the caller to select the intended one. Calculate closing value as exact nonnegative units times eligible unit cost, rounding once using the declared currency scale. Return the product, quantity, method actually applied and cost evidence ID. This is a product costing valuation and does not implement FIFO or LIFO flows. It changes neither the stock ledger nor any journal.

## Callers and migration

Extend the adapters invoking `record_product_cost`, `list_product_costs`, and `stock_valuation` in `rust/apps/api-server/src/records.rs`. Preserve the existing legacy responses. For historical rows keep originally reported values and classify them as legacy until the basis and period are verified. No backfill infers a method from a label or turns missing units into zero.

Say “Choose a product cost that applied on the valuation date” for no eligible cost, “This cost is stated in a different unit” for a quantity mismatch and “No unit cost can be derived from zero production” for a zero denominator.

## Acceptance tests

- In `rust/apps/api-server/tests/absorption_costing_http.rs`, add `stock_valuation_uses_effective_cost_beyond_legacy_list_cap` and `superseded_cost_revision_replays_the_original_value`.
- Add `product_cost_valuation_requires_currency_and_compatible_units` to the absorption module with exact fractional units.

Done when a reviewer can reproduce the valuation from the returned revision and no newer cost changes a pinned result.
