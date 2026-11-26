# Produce complete reconciliation and ageing reports

Baseline `728f9a22a5371f40fce7278c31566f8a074f2884`. Depends on the accounting and access safeguards in 02 and 03.

## Existing behavior

Both features already have Flutter screens. `rust/crates/domains/accounting/src/bank_reconciliation.rs::reconcile` reads up to 10,000 cash entries before filtering by account and status. It ranks date and reference, and has a final same-amount/same-direction match even when both differ. `aged_receivables.rs::aged_receivables` reads sales and receipts with a 5,000-row limit. At the baseline, the next consumer of that reader is `facts.rs::record_fact`, so incomplete ageing can affect a receipt decision as well as a report. Existing HTTP coverage is in `rust/apps/api-server/tests/bank_reconciliation_http.rs` and `aged_receivables_http.rs`.

## Data and API changes

Define a stable store query for all relevant rows in a Book at a stated cutoff. Use deterministic row IDs as tie-breakers and a consistent read snapshot when paging multiple tables. Apply account and date predicates before page limits on production providers, with a matching exact-money fixture on both Postgres and SQLite. A bounded request that cannot complete must return an explicit incomplete-data error and withhold approval and customer reminders.

In `bank_reconciliation.rs`, accept a statement end date, validate it against statement line dates, and compute the book balance from the matching account through that day. Make exact reference and compatible amount/direction a strong candidate. Include conflicting or equally plausible candidates in a review list with source IDs and dates. Prevent reuse of a posting and preserve unmatched items. An arithmetic difference of zero still leaves unresolved matches visible. Extend the contract at `POST /accounting/bank-reconciliation/match` and update `apps/flutter/lib/features/bank_reconciliation/data/bank_reconciliation_service.dart::BankReconciliationService` and its screen. Return a preview for older requests missing an end date, clearly marked unable to confirm.

In ageing, apply the cutoff to both invoices and receipts. Reserve explicit document allocations before applying unallocated amounts to the oldest eligible customer invoice. Keep customer credits separate and return unreadable source records by ID. Use Book money and exact decimals. An empty Book with an explicit as-of date returns a complete zero. Keep `facts::record_fact` on the same complete reader and refuse a settlement classification if its debt evidence is incomplete. Update `apps/flutter/lib/features/aged_receivables/presentation/aged_receivables_page.dart` to expose completeness and keep reminders disabled while a report is incomplete.

## Migration and errors

Retain historical working papers. A draft can be re-read against the cutoff and updated with the matcher version when confirmed. A previously saved statement without a cutoff remains readable as historical. Explain invalid cutoff, unreadable source rows and ambiguous matches in the response and return enough context for the person to correct them. No financial row is rewritten by recomputing a report.

## Acceptance tests

- `future_cash_entry_cannot_match_an_earlier_statement` and `duplicate_amounts_require_review` cover false positive risks.
- `reports_read_past_the_old_limits_on_both_providers` uses more rows than the previous fixed reads and compares results to exact expected balances.
- `later_receipts_do_not_change_historical_ageing` checks cutoffs and customer credit.
- `fact_settlement_refuses_incomplete_debt_evidence` proves a read problem cannot turn a receipt into a sale.
- Done when Rust HTTP and Flutter contract tests agree and the end-to-end Book fixture still closes with consistent reports.
