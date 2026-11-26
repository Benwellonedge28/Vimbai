# Work package 65 — Make scenario budgets traceable to explicit assumptions

Scenario-budget baseline [ca0a15a4](https://github.com/Benwellonedge28/Vimbai/commit/ca0a15a467b3263bf2ae5a5342ee30ca2120252e).

## Current scenario output

`analyze_scenarios` in `rust/crates/domains/budgeting/src/scenario_budget.rs` takes caller-provided revenue multipliers but applies fixed expense multipliers to the two alternatives. It compares the profit spread with base profit to produce a risk label. The `scenario-budget-analyze` registration in `rust/apps/api-server/src/lib.rs` calls `scenario_budget_from_json`. The separate stored scenario-modelling service is outside this package.

## Recorded assumptions drive a versioned comparison

For request version 2, accept exact nonnegative base revenue and expense in a declared currency and inclusive window. Require one base case and at least one named alternative. Each alternative has an opaque ID, display label, revenue multiplier, expense multiplier and a short assumption reference. Multipliers are nonnegative decimal strings, and one is the neutral factor. For the base case require both factors to equal one. Reject duplicate IDs and malformed or omitted factors. Keep presentation labels independent of ordering, and preserve the supplied case order in the response.

Multiply each baseline by the corresponding exact factor, apply a documented output precision once per displayed figure and derive displayed profit as displayed revenue less displayed expense. Return nullable margin when scenario revenue is zero. Return changes from the base to each case and min/max projected profit and spread. Identify the min and max cases by ID, with stable ID order for ties. A baseline loss is valid. Show the spread in currency units without assigning a risk level or claiming a probability.

For base revenue 100 and expense 80, an alternative with revenue factor 0.90 and expense factor 1.10 yields revenue 90, expense 88 and profit 2. Its profit change relative to the base is -18. If a named “best” case actually yields a lower profit, the arithmetic rank still follows the amount.

## Compatibility and failure messages

Add the version dispatch to `scenario_budget_from_json`, retain the legacy response keys for older requests, and keep calling the same registry entry. Return the exact input parameters, precision and calculation version alongside outputs so callers can store the working paper themselves. This calculation writes no records, posts no journals and requires no data migration. Historical scenario outputs cannot be treated as measured risk, and callers that retain them should show their calculation version.

Show “Provide a revenue and expense factor for every case” for missing input and “Use a different identifier for each case” for duplicates. Invalid baseline amounts or dates identify the field. An out-of-range precision request says “Choose a supported reporting precision.”

## Verification

- In the scenario budget module, add `scenario_expenses_follow_supplied_factor_and_base_loss_is_valid`. Compare a profitable and loss-making baseline.
- Add `scenario_zero_revenue_has_no_margin_and_labels_do_not_rank_cases` alongside it.
- Add `registered_scenario_budget_preserves_legacy_and_accepts_exact_factors` to the registry tests in `rust/apps/api-server/src/lib.rs`. Use a factor that would lose significant digits in the previous float input path.

Accept when a reviewer can derive every result from the returned assumptions and the versioned response cleanly distinguishes actual arithmetic from case labels.
