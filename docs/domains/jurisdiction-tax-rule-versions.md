# Use explicitly approved tax rules for each jurisdiction and effective date

Source baseline `2df3a7685b1109756fad0cca4d7b648b25790bdb`. Depends on 01, 02 and 10.

## Existing behavior

`rust/crates/domains/accounting/src/taxes.rs` owns Book tax settings and calculation. `tax_ledger.rs` in that crate reads tax balances and records settlements. `rust/crates/domains/taxation/src/rates.rs` includes a compatibility default table and picks the first active row covering the date. `rust/crates/domains/taxation/src/compliance.rs` records obligations and filing activity. Existing tax-ledger and statutory parity tests are under `rust/apps/api-server/tests/`. Those code paths establish accounting mechanics. Supported jurisdictions and requirements need separately reviewed data and fixtures.

## Rule version and calculation contract

Introduce a jurisdiction registry with reviewed, effective-dated tax-rule versions. Each version carries jurisdiction code, authority, authoritative source and revision date, tax code, applicable categories, decimal rate or formula parameters, inclusion rule, rounding method, civil-date convention and recoverability. Store Book assignments independently, with jurisdiction, effective date, permitted overrides, input/output/control-account mappings and the actor who approved each change. A rule selector returns one applicable version or a reason it cannot decide. Reject overlapping active versions within the same rule scope and rate precedence. An exemption or zero rate must be an explicit match with its own evidence.

Route Book document posting through `taxes.rs` with the selected version and exact-money calculation result. Persist rule identifiers, rate, amounts and rounding provenance beside the document and posting references. Bind returns to a Book, tax account, legal period and rule versions. Generate a report from these recorded facts and provide an exception list for incomplete classifications. Corrections use ledger reversals or adjustments linked to their original records and a separate return revision. Keep a filing acknowledgment as evidence of an approved submission when an external process exists.

## Compatibility and rollout

The legacy calculation surface retains its documented compatibility behavior during migration. Label such answers as estimates with source `legacy_default` where a default was used and prevent them from serving as evidence for an approved filing. Never re-rate historical posted documents silently. Capture resolved versions for new posts after assignment. An unconfigured Book can enter a draft, and posting or filing says “Set the tax rule and control account for this jurisdiction and date.” Ship one reviewed jurisdiction profile at a time. Obtain primary authority references and qualified local review before publishing any rate table or filing claim.

## Acceptance tests

- `date_boundary_applies_exactly_one_approved_rule` uses documents on each side of a transition and rejects an overlap.
- `legacy_default_cannot_approve_a_statutory_return` verifies compatibility and filing separation.
- `configured_zero_rate_is_distinct_from_missing_tax_policy` checks the explicit classification.
- `prior_period_correction_keeps_the_original_rule_evidence` ties revisions to the ledger.
- Done when one jurisdiction is backed by independently verified sample returns, the return reconciles to the tax control accounts, and a rerun against the same revision is reproducible.
