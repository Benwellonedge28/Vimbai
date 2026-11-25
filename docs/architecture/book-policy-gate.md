# Enforce the boundary between Book policy and engine logic

Baseline `728f9a22a5371f40fce7278c31566f8a074f2884`. Roadmap P12, ADR-014 and ADR-017.

## Existing behavior

`rust/xtask/src/safety.rs` runs source checks, and `rust/xtask/src/boundaries.rs::ledger_authority_outside_the_kernel` checks ownership of posting. `rust/crates/domains/accounting/src/measures.rs` stores measures as `BookMeasure` rows. `rust/crates/domains/accounting/src/progress.rs` computes a fixed compatibility response. The rule protecting Book policy still lacks its requested gate.

## Implementation

Add `rust/xtask/src/book_policy.rs` for the scanner and diagnostics. Register `xtask book-policy --check` in `rust/xtask/src/main.rs` and expose its check through the workspace test gate in `rust/xtask/tests/standing_rules.rs`. Scan production Rust under accounting, ledger and automation. Parse Rust syntax so strings in tests, documentation examples and commented code do not become policy violations. Treat malformed source or zero scanned files as a check failure.

Read the protected values from an explicit, reviewed fixture of representative account numbers, tax/currency identifiers and industry values. Detect literals in comparisons, match arms, policy assignments and lookup arguments. Exclude independently defined protocol constants such as the currency field name, date format and decimal zero. Permit literals only at explicitly listed paths that install defaults. Record grandfathered findings by relative path, symbol and normalized literal, with a written reason. Report stale entries when the corresponding code disappears. Keep the approved list under review each time a new Book-varying concept is introduced. The gate has precisely this detection scope, so retain behavior tests with renamed and differently numbered Books.

Enforce the measures boundary by inventorying the compatibility fields provided by `progress.rs` and failing on additions to that inventory. Verify with a behavior test that a new `BookMeasure` row appears in the measures response without editing the handler. Treat changes to the accepted six source primitives as ADR-017 changes.

## Failure and migration

Print file, line, rule and a concrete replacement such as “Read the account number from this Book’s chart.” A legacy exception with a changed symbol must be reviewed in the same PR. No customer data migration. The initial inventory happens in the same PR as the gate and identifies unresolved findings clearly.

## Acceptance tests

- `refuses_book_specific_literals_in_production_branches` places a protected value in an engine match arm and requires an actionable diagnostic.
- `accepts_default_data_tests_and_protocol_literals` isolates the permitted locations and tests.
- `fails_when_scan_is_empty_or_a_baseline_entry_is_stale` protects against vacuous passes.
- `adding_a_measure_row_requires_no_handler_change` exercises the persisted row through the measures reader.
- Done when the tests and preflight pass, the scanned roots are printed, every legacy finding has an owner and rationale, and CI fails when a new violation is introduced.
