# Work package 34 — Commit paid expense evidence with the journal

Proposed design against `dfd535cab7037f71de3c4ee1d1268b5c094f5683`.

## Current behavior
`create_expense` in `rust/crates/domains/accounting/src/expenses.rs` posts to the ledger before calling `cashbook::create_entry` for a paid expense and `remember` for the expense receipt. Cashbook creation itself moves the balance before creating its row. Any failure after the first write can separate the expense list, cashbook and general ledger.

## Transaction
Use the existing Book-scoped expense reference as the business key. Persist a versioned canonical request fingerprint and the journal and cash entry IDs in the expense receipt. Bind date, amounts, document currency/rate, payee, payment or payable choice, and supplied account selections. Preserve the entered record on failure. A matching retry returns the existing receipt before resolving mutable defaults. A different payload for the same key returns “This expense reference already belongs to another record. Review it before submitting again.”

Share the posting transaction with [opening balance receipts](opening-balance-receipts.md). In one commit write the ledger entry, expense receipt and, for paid expenses, exactly one cash row and its balance delta. An unpaid expense credits the configured payable and creates no cash entry. Use the existing Book and period gates, exact Money, and the stored currency conversion. The credit in Book currency and the cashbook base delta must reconcile. On mismatch tell the user to review the entered amount and exchange rate. Any future correction goes through authorised reversal.

## Callers and existing data
Keep `list_expenses` and the accounting routes in `rust/apps/api-server/src/accounting_book.rs` compatible. Update `apps/flutter/lib/features/expenses/data/expense_posting.dart` and `apps/flutter/lib/features/expenses/presentation/record_expense_page.dart` to preserve the submission reference after a lost response. For old receipts keep their IDs. Identify missing linked rows through the scoped reference and exact journal evidence. Report ambiguous evidence for accounting review, without speculative cash balance corrections.

## Acceptance
- `expense_receipt_fault_rolls_back_journal_cash_and_balance` faults each write boundary on SQLite and PostgreSQL.
- `paid_expense_currency_matches_cashbook_base_amount` covers a non-Book currency payment.
- `expense_replay_returns_the_original_links` races matching requests and then changes the reference payload.
- `unpaid_expense_never_creates_a_cash_entry` verifies the payable route.

Complete when the three paid-expense views share a stable journal reference and reconcile after a process restart.
