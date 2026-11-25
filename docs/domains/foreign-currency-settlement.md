# Settle foreign invoices and bills from their recorded carrying amounts

Source baseline `2df3a7685b1109756fad0cca4d7b648b25790bdb`. Depends on 02, 04 and ADR-012. Supplies reproducible inputs for 18.

## Existing behavior

`rust/crates/domains/accounting/src/document_currency.rs::in_book_money` converts a document into functional money. `fx_settlement.rs::settle_foreign` accepts caller-supplied carrying and settlement amounts and falls back to fixed account numbers for omitted mappings. `fx_revaluation.rs::revalue` implements the separate closing adjustment. The HTTP layer lives in `rust/apps/api-server/src/accounting_book.rs`. These surfaces should continue to agree with customer and supplier balances.

## Settlement contract

Add a document-backed command that accepts the document or authorised allocation IDs, amount in transaction money, actual payment amount and money, Book cash account, immutable rate record, effective date and idempotency key. Resolve document ownership and remaining foreign balance from the Book ledger/subledger. The engine derives the portion of the carrying amount. Preserve functional total exactly across instalments by assigning any residual to the final allocation under the Book rounding policy. Document the treatment of prior posted unrealised adjustments: carry their references and reverse or release the applicable portion through the authorised posting flow so the same gain is recognised once.

Resolve receivables, payables, money and gain/loss roles through 02 rather than trusting numeric defaults. Validate date, currency pair, quote direction, policy and period. Record the rate identity, payment basis, allocated amounts and ledger entry IDs in an immutable settlement receipt. Post through ledger-core. The document’s remaining balance, receipt, payment record and accounting entry commit as one operation. Reject an amount exceeding the open balance unless a separate owner-approved customer credit workflow covers it. Reports keep realised gain or loss, closing revaluation and presentation translation distinct.

## Backward compatibility and failures

Existing posted FX entries are historical and must not be repriced. Keep the prior endpoint’s compatibility behavior where required, label the manual input path, and switch normal Flutter settlement to the document-backed command after contract tests. Make a missing rate or account actionable, for example “Set a realised exchange loss account in this Book.” A closed period requires the existing dated correction or reopen process. Reversals reference the original settlement, rate and allocations.

## Acceptance tests

- `partial_receipts_and_the_final_residual_clear_one_foreign_invoice` checks exact money and replay.
- `nonstandard_chart_numbers_resolve_from_book_roles` covers invoice and bill on both sides of an exchange movement.
- `closing_revaluation_and_later_settlement_count_the_gain_once` compares balances and reporting categories.
- `a_missing_rate_or_overallocation_writes_nothing` verifies transaction and retry behavior.
- Done when the source documents, cash account, operating balance and FX result reconcile for both receivables and payables.
