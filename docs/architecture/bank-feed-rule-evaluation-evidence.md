# Work package 68 — Explain why a bank-feed rule matched

Design reviewed at [3a2c2ca1](https://github.com/Benwellonedge28/Vimbai/commit/3a2c2ca16978df88a95ad6b1c1decb2148a8707b).

## Current interpretation

In `rust/crates/domains/banking/src/bank_feed.rs`, `rule_matches` allows a missing merchant or category to satisfy an explicit condition. `matched_rule` sorts by priority and leaves ties to collection order. `apply_rules` skips unavailable transaction IDs and writes a matched rule and a confidence of one. The result has no saved rule revision or explanation.

## Deterministic evaluation

Validate conditions at creation and update. Require nonblank patterns, nonempty category sets, exact nonnegative absolute-amount bounds with lower no greater than upper, and at least one supported condition. Preserve case-insensitive literal substring semantics for merchant names. Require populated input for every configured predicate. A missing input makes the rule ineligible and produces an explicit reason in the evaluation. Sort eligible rules by descending priority, then by ascending stable rule ID. Record that ordering in the algorithm version. A matching rule gives a suggestion about imported evidence. It cannot authorize a reconciliation, post a journal or change the imported signed amount.

In a versioned batch, freeze the applicable rule revisions for the evaluated Book and each line's read version. Return one result for each input position, including duplicates in their original order and an identical unavailable result for missing or foreign IDs. Store at most one evaluation per transaction and rule-set revision. A write with changed transaction evidence fails with “This bank line changed. Run the rule again.” A later no-match evaluation supersedes the earlier suggestion for the new rule set while preserving the previous evaluation. Any failure must identify which IDs have durable results, so a retry can resume without hiding partial completion.

## Entry points and historical rules

Change `create_feed_rule`, `update_feed_rule`, `matched_rule` and `apply_rules` in the domain. Expose the versioned outcomes from `service_bank_feed.rs` and the records adapter calling `banking::bank_feed::apply_rules`. Keep the old response shape under ADR-004 and add per-line evidence to the opt-in response. An invalid rule names its condition, for example “Enter an upper amount at least as large as the lower amount.”

Read older rule definitions as stored. Compile only those that meet the new validation and report invalid rules as requiring adjustment. Prior matched-rule IDs remain historical associations and are not retroactively assigned revision evidence.

## Tests

- Add `missing_merchant_or_category_never_satisfies_a_required_condition` and `priority_ties_are_resolved_by_stable_rule_id` beside the domain matcher tests.
- Add `rule_batch_preserves_input_order_and_hides_foreign_line_identity` to `rust/apps/api-server/tests/bank_feed_http.rs`.
- Add `rule_rerun_keeps_historical_evidence_and_clears_stale_suggestion` to the same suite, checking the ledger and reconciliation status are unchanged.

Accept when a reviewer can reproduce the proposed match from the recorded rule revision and observed bank-line fields.
