# Work package 90 — Make overhead absorption rates carry their units

Overhead-rate baseline [5c2d0018](https://github.com/Benwellonedge28/Vimbai/commit/5c2d0018ed10d212ab43eda94a0a8d2509c3ea0d).

## Where interpretation can drift

In `rust/crates/domains/costing/src/overhead_absorption.rs`, `base_rate` selects routes whose output can be a per-unit rate or a percentage. `absorbed_overhead` accepts an amount and a raw rate and multiplies them without establishing its unit. `create_rate` stores both the basis name and calculated rate, but `rates` filters after applying a Book-wide cap.

## An exact rate contract

Represent a rate as an immutable scoped revision, with centre or approved blanket scope, effective period, currency, denominator unit, numerator overhead and exact denominator. A unit basis uses overhead divided by physical units. A monetary basis stores a dimensionless fraction and renders its percent as the fraction times 100. A zero denominator produces an unavailable rate and an explanation. For a verified application require the rate revision, effective date, actual measured base and matching unit. Compute applied cost from the exact fraction or per-unit rate and round once at Book precision. Return the frozen inputs and the unit used. Never infer percentage interpretation from magnitude. For example, overhead 250 against monetary base 1,000 is a fraction of 0.25, displayed as 25 percent, and applied to 60 as 15.

## Routes and transition

Extend `create_rate`, `base_rate`, `blanket_rate`, and `absorbed_overhead`, with `records.rs` resolving scoped rate IDs before domain calculation. Keep published basis-specific response keys for legacy clients. Filter rates by centre and date before paging. Old results remain readable and may receive a verified unit only when their source basis establishes it. No automatic rewrite or posting occurs.

For a wrong basis say “This rate uses a different unit from the actual base.” For a zero denominator say “Enter a positive budgeted base to calculate a rate.” For an absent revision say “Choose a rate in force for this centre and date.”

## Tests

- In the rate module add `monetary_percentage_and_physical_unit_rates_apply_differently` and `zero_budgeted_base_does_not_create_applicable_rate`.
- In `rust/apps/api-server/tests/costing_prefixes_http.rs`, add `overhead_application_verifies_rate_revision_scope_and_units`, with a revised and a foreign rate.

Accept when a displayed percent cannot be applied as a raw multiplier and a reviewer can reproduce the exact absorbed amount.
