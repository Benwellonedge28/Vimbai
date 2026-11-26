# Work package 36 — Record tax remittances with consistent liability evidence

Proposed design against `dfd535cab7037f71de3c4ee1d1268b5c094f5683`.

## Current behavior
`settle_tax` in `rust/crates/domains/accounting/src/tax_ledger.rs` reads the amount owed, rejects amounts exceeding it, posts the journal and returns the calculated `owed_after`. Requests with different references can compute from the same prior liability. That check needs to hold when the write commits.

## Commit contract
For a new recording, take the Book, effective date, resolved tax and money accounts and exact amount. Under a Book-and-tax-account transaction guard, re-read the liability with the same `owed_on` semantics used today. Include other writes affecting that tax control balance in the same concurrency guarantee. Preserve the existing overpayment and reclaim distinction. If an intervening posting leaves insufficient liability, report “The amount owed has changed. Review the current tax balance before recording this payment.” Reject a closed period, missing account, or ambiguous tax selection before any new posting. Tax settlement here records money already paid outside Vimbai.

Use the supplied reference or the current derived reference for replay identity within the Book. Bind it to the canonical submitted payload. Store resolved account evidence, `owed_before`, `owed_after`, and journal ID atomically with the debit tax liability and credit money-account entry. A matching replay returns the original receipt even when liability subsequently changes. A conflicting request under the same reference returns “This tax payment reference already belongs to a different record.” Present the recorded balance after the transaction separately from a refreshed live balance.

## Consumers and upgrade
Keep the adapter `settle_tax` in `rust/apps/api-server/src/accounting_book.rs` and the fields used by `apps/flutter/lib/features/tax/data/tax_ledger.dart`. Update `apps/flutter/lib/features/tax/presentation/tax_ledger_page.dart` to retain a retry's reference and refresh on insufficient balance. Historical journals remain readable. A journal with no recoverable receipt is reported as historical evidence. Do not infer a historical `owed_after` from today's balance.

## Acceptance
- `concurrent_tax_recordings_do_not_overpay_one_liability` posts different references against the same balance on both stores.
- `tax_recording_retry_returns_original_owed_after` adds a later liability entry before replay.
- `tax_receipt_fault_leaves_owed_balance_unchanged` injects failure between intended writes.
- `tax_conflict_preserves_form_and_refreshes_liability` verifies the Flutter state.

Complete when the receipt and journal survive restart together and a conflicting write produces no ledger change.
