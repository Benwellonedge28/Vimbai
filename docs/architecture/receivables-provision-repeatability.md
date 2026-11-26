# Work package 86 — Calculate a provision repeatedly without changing debtor age

Provision source pinned at [5c2d0018](https://github.com/Benwellonedge28/Vimbai/commit/5c2d0018ed10d212ab43eda94a0a8d2509c3ea0d).

## Existing write-on-calculation

In `rust/crates/domains/receivables/src/provision.rs`, `calculate_provision` adds elapsed days to each saved debtor age and writes the result back. Calling it again can age the same input again. `create_journal` calls the calculator without the selected policy and builds a stored journal proposal using fixed account codes. This package covers the receivables assessment service, alongside the [accounting provision posting design](doubtful-debt-provision-concurrency.md).

## Immutable calculation inputs

For the versioned assessment require a reporting date and an approved dated policy revision. Take each debtor's verified due date and exact exposure effective on that date. For a historical row with an evidenced prior age and its observation date, derive its age by a single signed day difference from that observation. A missing age basis is an exception. Cap negative age at zero for the current bucket. Validate nonoverlapping ordered policy bands, explicit percentages between zero and 100, and complete policy coverage. Aggregate exact per-debtor results in Book currency, using a declared publication precision. Compare an optional caller control total with the measured exposure and surface any variance explicitly.

A preview is read-only. A saved assessment keeps policy ID, cutoff, source watermark, per-debtor ages and exact calculated amounts. A journal proposal references that exact assessment. It cannot invoke calculation again with defaults. Resolve the Book's own expense and allowance roles. For an increase, propose debit expense and credit allowance. For a decrease, propose the exact reverse. A zero change produces no lines. This proposal is never a posting and the accounting command still enforces period and ledger invariants.

## Routes and migration

Update `calculate_provision`, `create_journal` and the handlers calling them in `rust/apps/api-server/src/records.rs`. Keep legacy projections visible but distinguish them from reproducible assessments. Preserve old debtor ages and journal proposals. A reviewer establishes the actual effective basis before classifying a historic debtor as verified. A preview without evidence says “This debtor has no verified starting date or age.” A missing selected policy says “Choose an approved provision policy for this date.”

## Verification

- Add `provision_preview_repeats_without_updating_debtor_age` in the provision module.
- Add `provision_proposal_uses_saved_policy_for_increase_and_release` with a nondefault policy and exact inverse.
- In `rust/apps/api-server/tests/records_http.rs` add `provision_preview_exposes_missing_age_basis_and_respects_book_scope`.

Complete when the same evidence and policy reproduce the same output and preview leaves the debtor register untouched.
