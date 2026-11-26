# Work package 37 — Preserve exact amounts in saved cash-flow statements

Proposed design against `f9795238b56a8a02c8c7513cc72e484f69ff9b2d`.

## Existing path
`rust/crates/domains/accounting/src/cash_flow.rs::generate` accepts `beginning_cash` as a string or converts a JSON number through `as_f64`. It saves opening and closing totals with the three activity sections. The generate, latest and history adapters in `rust/apps/api-server/src/records.rs` call this module.

## Amount contract
Audit every monetary field read by `generate` and its `section` helper. Decode decimal strings with Money. Accept legacy integral JSON numbers only when their integer representation is exact. Fractional JSON numbers require an exact decimal string because binary rounding can change the submitted amount before it reaches Money. Give a field-specific message such as “Enter beginning cash as a decimal string, for example 1250.40.” Missing optional opening cash retains its existing zero default. Treat null or malformed supplied values as errors. Validate all sections before saving anything.

Compute each signed subtotal in Money and derive closing cash from the opening balance plus the three subtotals. Preserve the existing six-decimal representation. Reject inputs that would lose meaningful precision when persisted at that scale, and explain which field needs rounding. Check the reconciled equation again at the persistence boundary. Retain the current scope and report record. Generating a statement creates no ledger movement.

## Compatibility and verification
Keep saved statements readable through `read_back`; historical rounding cannot be reconstructed from already stored totals. Audit clients of the generate adapter for numeric writes. The ledger-derived Flutter statement page at `apps/flutter/lib/features/accounting/presentation/cash_flow_statement_page.dart` should only change if its service uses this input route.

- `cash_flow_preserves_a_decimal_beyond_binary_integer_precision` verifies exact opening and closing values after reload.
- `cash_flow_refuses_inexact_numeric_input_before_saving` tests the opening amount and nested section lines independently.
- `cash_flow_legacy_statement_still_reads` verifies existing persisted rows with six-decimal strings.

Done when SQLite and PostgreSQL return identical section totals for the same decimal inputs, and the scoped history stays empty after an invalid request.
