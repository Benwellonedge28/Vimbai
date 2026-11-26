# Resolve the account a posting needs

Baseline `728f9a22a5371f40fce7278c31566f8a074f2884`. Depends on 01 and ADR-002, ADR-013, ADR-015.

## Existing behavior and callers

`rust/crates/domains/accounting/src/facts.rs::record_fact` resolves facts against the Book chart and has a read-only rehearsal. `rust/crates/domains/accounting/src/ledger_book.rs::create_account`, `create_entry`, `post_draft` and `reverse_entry` expose the journal lifecycle. `rust/apps/api-server/src/records.rs::post_fact` and Flutter’s `apps/flutter/lib/features/facts/data/fact_posting.dart` carry that contract. The review base `2384123` also carries the settlement result into Flutter through `FactAnswer.settlesDebt`; keep that response behavior for posted entries and rehearsals. The end-to-end fixture `rust/apps/api-server/tests/accounting_end_to_end_http.rs` checks the figures after the Book trades and closes.

## Resolution and atomicity

Place one typed resolver beside `ledger_book.rs` and call it from `record_fact` and each subsidiary journal that currently selects accounts. Inputs are the Book scope, effective date, the posting intent, expected account kind, and any Book-owned role or explicit account reference. An exact eligible match wins. Multiple eligible matches refuse with their labels. Reserve auto-creation for explicitly eligible ordinary accounts whose type is inferable and whose Book has a configured numbering band. Control roles for receivables, payables, tax, inventory, fixed assets and suspense require a configured mapping. A valid configured bank/cash account remains the source or destination for money movement recorded in the Book.

Return a resolution plan from rehearsal without assigning a durable number. At commit, re-read the Book policy, check the period and idempotency key, reserve an unused number within the configured band, and invoke the ledger posting authority. In `Graph` in `rust/crates/adapters/database/src/lib.rs`, add a semantic transaction that commits the provisional chart row, posting and relationships together on Postgres and SQLite. It must have a single committed result for retries, including concurrent attempts by the same caller. A provider without that transaction must refuse provisional creation. Keep the accounting validation in ledger-core. Make provisional state visible in `ledger_book::list_accounts` and report labels. Reclassify using a posted transfer with an audit reference to the source history.

## Errors and migration

Give specific responses for ambiguous names, missing control mappings, absent/exhausted bands and closed periods. For example, “Set the receivables control account for this Book before recording this receipt.” Preserve input on the phone. Existing charts keep their numbers and types. Add optional Book configuration and provisional metadata with a default of false for existing accounts. Legacy Books without bands continue to resolve existing accounts and can enable provisional creation after configuring bands. Never renumber on upgrade.

## Acceptance tests

- `rehearsal_and_rollback_leave_the_chart_unchanged` confirms dry-run and forced failed-post behavior.
- `concurrent_retries_create_one_provisional_account_and_one_entry` runs on both supported stores, including after a lost response.
- `missing_control_mapping_refuses_without_writing` covers every protected role.
- `reclassification_posts_a_transfer_without_rewriting_history` verifies journal links and balanced totals.
- Done when all posting paths in the caller inventory use the same resolver, the existing end-to-end reports agree and store conformance covers the new operation.
