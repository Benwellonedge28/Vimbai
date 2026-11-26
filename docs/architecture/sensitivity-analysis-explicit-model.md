# Work package 66 — Measure sensitivity from a declared model

Proposed against source [ca0a15a4](https://github.com/Benwellonedge28/Vimbai/commit/ca0a15a467b3263bf2ae5a5342ee30ca2120252e).

## The display name currently changes the calculation

In `rust/crates/domains/budgeting/src/sensitivity_analysis.rs`, `estimate_impact` infers coefficients from words in a variable's name. `analyze` ranks by average elasticity formed from the reciprocal of the relative target change divided by the input change. Unparseable step values can become zero. The stored analysis does not preserve a declared relationship that a reviewer can reapply.

## Fixed inputs and one-variable steps

Support version 2 as an explicitly linear model. A request supplies the target's identity and currency, its exact base amount, and each variable's opaque ID, unit, exact base and signed coefficient. The coefficient is the target currency change for one unit of that variable. Attach a source reference for each coefficient. Variable labels do not participate in arithmetic. Require a unique, nonempty list of step percentages as decimal strings and at least one nonzero step. The model accepts a nonblank unit per variable. Base, delta and coefficient all refer to that same unit, with no implicit unit conversion. Reject a coefficient whose declared denominator unit disagrees with its variable, duplicate variable IDs and malformed numbers.

For every variable and step, hold all other inputs fixed. Compute input delta as base times step percent divided by 100, then target delta as coefficient times input delta. Return the changed variable, resulting target, signed delta and nullable elasticity. Where both baseline magnitudes and input delta are nonzero, elasticity is the absolute value of relative target change divided by relative input change. A zero sensitivity with nonzero denominators is zero. Where a denominator is zero, the result is unavailable. Rank by the largest absolute target change across the same requested steps, with stable ID order as a tie-breaker. If no target changed, return no leading variable.

For a target of 500, a variable base of 20, coefficient -3 and a 10 percent step, the variable moves to 22 and the target to 494. Renaming the variable leaves both figures intact. Report Money values as exact strings, with explicit display scale. Apply rounding only at serialization, retaining enough model precision for reproduction.

## Store and read the frozen model

Extend `run_sensitivity` and `sensitivity_analyses` in `rust/apps/api-server/src/records.rs` under both existing prefixes. Authorize the company reference in the current Book, using the same scoped identity for lookup and filtering. Persist the model revision, coefficient evidence, exact inputs and derived outputs together in `SENSITIVITY_ANALYSIS`. A rerun is a new analysis and cannot overwrite the original assumptions. No accounting records are posted.

Keep historical IDs and display the old rows as legacy heuristic results. Do not use prior labels to infer approved coefficients or backfill rankings. For missing coefficients say “Provide the target change per unit and its source for each variable.” For an invalid step say “Enter a valid percentage for this step” and identify it.

## Evidence for release

- Add `renaming_sensitivity_variable_cannot_change_model_result` beside the budgeting tests, with exact positive and negative coefficients.
- Add `sensitivity_ranks_target_delta_with_zero_baseline_handled` to the same module. Include a larger and a smaller effect and a tie.
- Add `sensitivity_model_round_trips_without_cross_book_lookups` to `rust/apps/api-server/tests/records_http.rs`. Check the second route alias and retained historical results.

Release when a reviewer can reproduce all results from the stored model, and ranking follows the observed effect of the supplied steps.
