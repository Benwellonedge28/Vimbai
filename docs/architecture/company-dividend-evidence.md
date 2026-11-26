# Work package 51 — Preserve dividend allocations and evidence of payment

Proposed against [2c0642c](https://github.com/Benwellonedge28/Vimbai/commit/2c0642cd994674e4d455e93d524031eb6477e9a1).

## Current declaration and closing behavior

`declare_dividend` in `rust/crates/domains/accounting/src/company_accounting.rs` writes the dividend header and then builds holder payments from `kept(..., SHAREHOLDER, ...)`, which filters after a capped Book-wide read. The header uses the supplied withholding, while every holder payment uses a fixed ten percent. `pay_dividend` updates the header before updating holder rows and also reads a capped Book-wide set. Both functions return suggested journals. Neither posts through ledger-core. `dividend_payments` and `dividend_history` are the reads a reviewer needs to reconcile those amounts.

## Freeze an approved allocation

Introduce an evidence-backed declaration with a company, class, record date, declaration date, resolution reference and exact per-share amount in Book currency. Require an approved snapshot of entitled holder IDs and share counts as of the record date. The snapshot may be submitted from a dated register with its evidence reference. Current holdings can support a snapshot only if the reviewer certifies that they reflect the record date. Resolve the company, class and holders in the selected Book, require unique holder IDs and positive share counts, and sort by holder ID before calculating the receipt fingerprint. Reject declaration dates before the record date and enforce the normal posting-period check.

For each holder calculate gross as shares times the approved rate per share, using exact `Money`. Accept a reviewed withholding amount per holder with its rule or evidence reference. That amount must be between zero and gross. A zero amount needs the same evidence of the decision as a positive amount. Net equals gross less withholding. Derive header gross, withholding and net exclusively by adding those frozen rows. Reject supplied control totals that differ from the derived totals. Require the approved allocation to express each monetary amount exactly at the Book's supported posting precision. If the approved rate produces a fraction beyond that precision, return the affected holder IDs for a revised allocation policy before posting.

Reserve the resolution reference within the Book and company. Store the declaration header, frozen allocation, reviewer, snapshot reference, canonical input digest and full posting ID together. Debit the configured distribution equity account and credit the dividend payable for gross. Use Book-owned account configuration, `ledger_book::create_entry` and the transaction in [package 22](opening-balance-receipts.md). Mark the new dividend declared only when all those writes commit.

## Record a completed external batch

Accept one completed batch per evidence-backed declaration, with a date, immutable external evidence reference, and exact net amounts by holder. The batch date must be on or after the declaration date and in an open period. Match every frozen holder with positive net exactly once, reject unexpected holders and differing amounts, and allow a zero-net holder to complete without external cash evidence for that holder. Use independent settlement authorization and retain the authenticated actor. Record a debit to dividend payable for gross and credits to the confirmed Book bank and configured withholding payable for net and withholding respectively. Omit zero-value lines. The command records evidence after settlement. It has no connection to a payment provider.

In the same transaction, lock the declaration by scope and ID, read all frozen rows by dividend ID, post the settlement journal, write the settlement receipt and mark the header and allocations complete. A replay with the same external evidence, date, account selection and amounts returns the same receipt before applying current settings. Changed evidence on a completed declaration returns a conflict. Competing writes must converge to one settlement journal. Corrections require authorized ledger reversals linked to the original evidence. A second settlement is refused until the correction workflow has a reviewed state transition for it.

## Connect the routes and preserve prior records

Add `POST /company-accounting/dividends/verified-declaration` and `POST /company-accounting/dividends/:dividend_id/record-settlement`, plus the records aliases, in `rust/apps/api-server/src/records.rs`. Register them in the API inventory. Keep the legacy declaration and pay signatures during transition. The legacy pay handler must route evidence-backed rows to the settlement entry point by returning an actionable conflict. Update `dividend_payments` and `dividend_history` to expose evidence status and journal IDs and to apply scope and owner filters before pagination.

Keep historical header and holder rows as entered. Report a header-total mismatch, incomplete holder list, or missing posting link as a review item. A historical posting link can be attached only with a unique journal match and approved allocation evidence. Never recalculate historic withholding from current policy or create a second journal to fill an evidentiary gap. Maintain historical and verified totals as distinguishable fields where a read combines both kinds of record.

Show “The shareholder allocation does not match the approved dividend totals” for control-total differences, “This company’s dated shareholder register needs review” for missing or incomplete entitlement evidence, and “The settlement evidence does not match the declared net amounts” for differences in the external batch. A duplicate with changed evidence says “This dividend already has a different recorded settlement.” An interrupted commit says “The dividend records and journal could not be saved. Retry the same submission.”

## Evidence required before release

- In `company_accounting.rs`, add `dividend_header_is_the_sum_of_frozen_holder_allocations`. Use holders with different withholding amounts, zero withholding and unrelated holders in another company. Reject a supplied header that differs by the smallest stored unit.
- For cross-provider verification, add `dividend_allocation_and_journal_survive_or_rollback_as_one` to the accounting integration target in [package 47](subsidiary-journal-posting-receipts.md). Inject failure before the last holder write and after journal creation on both providers, and verify the previous state after restart.
- In `rust/apps/api-server/tests/records_http.rs`, add `dividend_settlement_replays_once_and_legacy_pay_cannot_complete_verified_row`. Race two settlement requests, retry under the records alias, and confirm a changed evidence reference fails without writing.
- Add `legacy_dividend_mismatch_stays_visible_without_backfilled_money` to the company-accounting tests. Use inconsistent header and holder amounts, then verify the discrepancy and original records are still present.

Accept when both posting IDs, the resolution and the external batch evidence can be traced from the same declaration, and every holder and header amount can be reconstructed using the frozen allocation.
