# Work package 30 — Reconcile payroll runs with their posted journals

Source baseline `e85043f61d13a2bef8f0d9fcdc066c559c09c3e6`. Proposed design.

## Current behavior

`post_run` in `rust/crates/domains/accounting/src/payroll.rs` posts using the caller's reference and then creates the payroll-run row. `list_runs` reads run rows. An interrupted write can therefore hide a posted journal from the payroll run list.

## Posting and retry contract

Use the trimmed payroll reference within the Book and payroll operation. Bind it to period, effective date, canonical amount breakdown, notes, and submitted account selectors. Store the resolved account IDs and journal ID as evidence. Exclude transport metadata and timestamps from comparison. A matching authorised replay returns the original run and does not resolve today's default account configuration again. If details differ, return “This payroll reference already identifies a different run. Review it before using another reference.” Preserve support for existing references and avoid inferring that one calendar month permits only one payroll batch.

For a fresh run, validate existing period and account rules, compute gross, withholding, net and employer cost with exact Money, and commit the run in the same transaction as the ledger post. Preserve debits to the applicable expense accounts and credits to matching liabilities. The journal records the obligation only. Remittance and actual salary transfers are outside the operation. A correction is a ledger reversal through the normal permission and date checks.

## Existing runs and clients

Retain existing row IDs. A row with complete stored evidence can be recognised as an existing receipt. Where the original command cannot be reconstructed from a historical journal, give read-only journal evidence to authorised users and mark the run as requiring reconciliation. Refuse to treat an ambiguous request as a safe replay. Leave the original journal and its numbers intact.

Keep the routes in `rust/apps/api-server/src/accounting_book.rs` and current fields consumed by `apps/flutter/lib/features/payroll/data/payroll_service.dart`. Update `apps/flutter/lib/features/payroll/presentation/payroll_page.dart` to retain the reference and entered amounts after an uncertain response. On an existing-run conflict, present the reference and direct the user to the payroll run list.

## Verification

- `payroll_receipt_failure_does_not_post_payroll` proves atomic rollback on SQLite and PostgreSQL.
- `payroll_retry_uses_original_account_evidence` changes the account defaults after the first successful request and checks replay returns the original journal.
- `same_month_distinct_payroll_references_can_coexist` verifies the existing reference boundary with separate legitimate runs.
- `payroll_reference_payload_conflict_keeps_original_run` verifies refusal through the route and checks trial balance totals.

Accept when the run list shows the same journal ID on restart and retries never create duplicate obligations.
