# Work package 35 — Link a bad-debt write-off to the invoice it reduces

Proposed design against `dfd535cab7037f71de3c4ee1d1268b5c094f5683`.

## Current behavior
`write_off` in `rust/crates/domains/accounting/src/bad_debts.rs` records a debtor label and posts debit bad-debt expense and credit receivables. Its reference combines date and debtor label. `aged_receivables` in `rust/crates/domains/accounting/src/aged_receivables.rs` reads cash receipts as allocation evidence. A debtor label alone cannot establish which invoice the write-off should reduce.

## Allocation contract
Extend the supported write-off command with invoice ID and an opaque operation ID. Resolve the invoice and receivables account in the active Book. Determine the open amount as of the effective date using the same invoice allocation inputs as ageing. Apply an exact partial or full write-off up to that amount. Hold a transaction guard shared by receipts and write-offs for the affected invoice and recheck its open amount at commit. A missing invoice returns “Choose an invoice from this Book.” An exhausted balance returns “That amount exceeds what remains open on this invoice.” A closed date keeps the existing period refusal.

Commit the balanced ledger post, allocation and write-off receipt together, retaining the original invoice and journal references. A replay returns the recorded result. Ageing applies the verified write-off only on or after its date. A later reversal restores the corresponding open amount on or after the reversal date and carries the inverse ledger entry. Preserve Book-currency Money and existing foreign-currency settlement rules. Limit initial invoice-linked support to Book-currency invoices pending explicit allocation rules for other currencies.

## Compatibility
Change `write_off`, `list_written_off`, `aged_receivables`, and the write-off adapter in `rust/apps/api-server/src/accounting_book.rs`. Preserve label-only history as unallocated. Keep the legacy command shape for existing callers and return a visible unallocated status on that path. An authorised accountant may link a legacy row only after reconciling its journal and invoice evidence. Ambiguous labels produce no inferred allocations.

## Acceptance
- `writeoff_reduces_only_the_selected_invoice_on_or_after_its_date` checks an earlier report and a second invoice for the debtor.
- `concurrent_receipt_and_writeoff_cannot_overallocate_invoice` runs on SQLite and PostgreSQL.
- `reversing_writeoff_restores_open_amount_without_editing_invoice` verifies the dated report and ledger.
- `legacy_unallocated_writeoff_does_not_guess_an_invoice` covers duplicate labels and cross-Book IDs.

Complete when the receivables control movement and invoice-level ageing change reconcile for each linked operation.
