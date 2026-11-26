# Work package 27 — Recover an accrual reversal by its journal ID

Source baseline `e85043f61d13a2bef8f0d9fcdc066c559c09c3e6`. Proposed design.

## Current behavior

In `rust/crates/domains/accounting/src/accruals.rs`, `reverse_accrual` reads the accrual, resolves the requested date (defaulting to the first day of the following month), checks the period, invokes `ledger_book::reverse_entry`, and then stamps the accrual row. A failed stamp can leave the list in disagreement with the ledger. `list_accruals` is the read surface to reconcile.

## Transition contract

Use the Book and accrual ID as the command identity, and the resolved reversal date as its immutable input. Load the accrual and original journal in the same Book. Once an authorised matching command commits, return the stored reversal ID and date for retries, including after that month closes. A different date after an existing reversal returns “This estimate was already reversed on [date]. Review the journal before making another correction.” For fresh reversals, retain the existing date default, open-period check and normal ledger reversal operation. Missing or inaccessible accruals use the existing scoped lookup response.

Commit the ledger reversal and row update together using the store transaction boundary from [posting account resolution](posting-account-resolution.md). Hold the provider guard through validation and commit so two sessions cannot reverse concurrently. A receipt or journal write failure rolls back the whole transaction. The receipt retains the original journal link and the reversal carries equal and opposite Money lines. Bill posting stays a separate command.

## Callers and compatibility

The reverse and list handlers in `rust/apps/api-server/src/accounting_book.rs` keep their routes and the existing `reversal_entry_id` and `reversed_on` fields. Update `apps/flutter/lib/features/accruals/data/accrual_posting.dart` and `apps/flutter/lib/features/accruals/presentation/a_cost_not_yet_billed_page.dart` to retain the requested date on uncertain failures and present a recovered receipt as success.

Keep legacy IDs. Where a legacy row lacks a stamp, verify a single reversal through the original ledger entry's reversal relationship and exact lines before repairing the projection. If the reversal cannot be verified, retain the row and show “This estimate's reversal could not be verified. Review its journal.” Never generate a replacement automatically.

## Proof

- `accrual_reversal_receipt_failure_leaves_original_active` injects failure after the reversal write on SQLite and PostgreSQL.
- `concurrent_reverse_accrual_requests_return_one_reversal` proves same-date replay and different-date refusal on both providers.
- `accrual_list_reconciles_a_verified_legacy_reversal` checks a verified missing stamp, an ambiguous legacy record, and another Book.

Accept when restarting after a lost reply preserves the original and exactly one linked reversal, and the accrual reader returns the same receipt as the command.
