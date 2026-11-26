# Work package 55 — Reconcile cash and bank separately in a receipts and payments paper

Design reviewed at [ca0a15a4](https://github.com/Benwellonedge28/Vimbai/commit/ca0a15a467b3263bf2ae5a5342ee30ca2120252e).

## What the existing worksheet calculates

`rust/crates/domains/accounting/src/bookkeeping.rs` holds `create_receipts_payments` and `list_receipts_payments`. Creation adds every receipt and subtracts every other entry type from bank, leaving closing cash equal to opening cash. It echoes `isContra` into the saved line but gives it no effect. Unknown categories can appear in total receipts or payments without appearing in a category subtotal. This saved worksheet is independent of the posted cash flow and cashbook readers.

## Give each line one effect

Use the opt-in `calculation_version: "2"` input on the existing route. Require an inclusive period, exact opening cash and bank balances in Book currency, and an explicit entries array. Each row has a unique opaque input ID, effective date in the selected period, positive exact amount and optional reference to external evidence. Apply the [shared precision rule](statement-of-affairs-revisions.md#save-a-dated-working-paper) to opening balances and entries. Labels and account names remain descriptive Book data. Preserve an overdraft as a signed bank balance. Refuse a negative opening cash amount, or a negative cash balance during the sequence, with the line that needs an opening balance or missing inflow correction. Compute in effective-date order with stable submitted order within a date.

| Entry | Required classification | Cash and bank effect |
| --- | --- | --- |
| `receipt` | One destination `cash` or `bank` and category `capital` or `revenue` | Increase only the named balance and include the amount in external receipts and its category |
| `payment` | One source `cash` or `bank` and category `asset`, `liability` or `revenue` | Decrease only the named balance and include the amount in external payments and its category |
| `contra` | Distinct source and destination among `cash` and `bank`, with supporting reference | Decrease one balance and increase the other by the same amount, with zero effect on external totals |

A transfer is one input line. Reject duplicated transfer evidence within one paper. Keep `is_contra` as derived response metadata. In the new version fail validation for a flag that conflicts with the type. Derive every subtotal and both closing balances in one pure calculation before persisting the `RECEIPTS_PAYMENTS` row. The sum of closing balances must equal opening cash plus opening bank plus external receipts less external payments. Report transfer totals separately by direction.

For opening cash 200 and bank 900, a cash receipt of 40, a cash-to-bank transfer of 100 and a bank payment of 60 yield cash 140 and bank 940. The 100 is visible as a transfer and the combined balance is 1,080.

## Delivery and existing records

`create_receipts_payments` and `list_receipts_payments` in `rust/apps/api-server/src/accounting_book.rs` expose the option on both existing prefixes. Reuse current list response fields and add the calculation version and transfer totals for new records. Keep the existing unversioned submission behavior under ADR-004. The worksheet records amounts the preparer has already observed. Opening or closed ledger periods can both be described. No cashbook row, transfer instruction or journal is produced.

Keep older rows and their outputs as stored. Cash or bank attribution cannot be recovered reliably from the old `account_name` label or from the contra flag. Offer a fresh versioned paper linked to its predecessor after the user supplies each missing source, destination or classification. Display “Choose whether this receipt went to cash or bank” for missing receipt attribution, “Enter the two sides of this cash-to-bank transfer” for incomplete contra data, and “This entry has no supported classification for its direction” with the affected row ID for mismatched categories.

## Independent checks

- Add `receipts_payments_reconciles_both_balances_without_counting_contra_in_turnover` to `bookkeeping.rs` tests. Exercise both transfer directions, a bank overdraft and each applicable external subtotal.
- Add `receipts_payments_refuses_a_duplicate_transfer_and_impossible_cash_sequence` to the same module. Confirm the refusal names the invalid row and writes no paper.
- Add `receipts_payments_legacy_and_versioned_papers_read_through_both_aliases` to `rust/apps/api-server/tests/records_http.rs`. Submit both versions and read the exact stored totals back while verifying the ledger and cashbook counts are unchanged.

Release when each saved external item contributes to exactly one subtotal, each transfer has zero combined effect, and the saved cash and bank closing balances can both be recomputed from the original inputs.
