# Work package 41 — Verify the cutoff and evidence for FX revaluation

Proposed design against `f9795238b56a8a02c8c7513cc72e484f69ff9b2d`.

## Existing path
`rust/crates/domains/accounting/src/fx_revaluation.rs::revalue` derives a month from `entry_date`, checks whether the month is closed and builds a ledger entry with reference `FX-{period}`. It computes differences through `restatement` and supplies the resulting journal ID to the adapter in `rust/apps/api-server/src/accounting_book.rs`.

## Reviewed command
Require the effective posting date to be the final calendar day of the requested month and validate it before preparing any journal. Use the repository's period helpers. Return “Choose the last day of the month being restated” for an otherwise valid date inside that month. Retain the closed-period refusal. A historical month that was closed cannot be silently shifted to an open date.

Persist a receipt with effective month, exact closing rate, the reviewed foreign balance and carrying amount for each account, calculated difference and journal ID. Bind the inputs to the Book and the existing period reference. Keep the existing debit and credit logic and gain/loss mappings. A no-difference result saves no journal and must allow a later reviewed submission if its inputs changed. Commit the receipt and posting atomically, and check the month lock at commit. A matching retry returns the journal receipt. Conflicting inputs for an already posted period return “This month already has a restatement. Review the posted journal before recording a correction.” A correction uses the existing reversal authority.

## Boundaries and upgrade
Continue treating the submitted balance set as explicit reviewed input. Preserve any existing journal and period reference. Historical journals without a receipt remain available with evidence marked unavailable. Never fabricate their closing rate. Keep the current success fields and add the receipt reference to the HTTP response. This design does not alter the separate foreign-currency settlement path.

## Acceptance
- `fx_revaluation_rejects_non_month_end_without_writes` covers February in leap and ordinary years and a valid mid-month date.
- `fx_revaluation_retry_returns_original_receipt` verifies input mismatch refusal and unchanged gain/loss.
- `fx_revaluation_and_month_close_serialize` exercises both transaction outcomes on SQLite and PostgreSQL.
- `legacy_fx_journal_remains_readable_without_rate_evidence` checks compatibility.

Done when a posted period can be traced to its exact rate and balance inputs after restart, and a failed receipt write leaves no posting.
