# Work package 56 — Preserve the evidence behind single-entry earnings estimates

Proposed specification pinned to [ca0a15a4](https://github.com/Benwellonedge28/Vimbai/commit/ca0a15a467b3263bf2ae5a5342ee30ca2120252e).

## The two existing entry points

`create_single_entry_conversion` and `create_profit_estimation` in `rust/crates/domains/accounting/src/bookkeeping.rs` share `earnings_from_capital`. The conversion saves all six opening and closing debtor, creditor and cash fields as zero even if a caller supplied values. Its optional receipts and payments totals also default to zero. The estimation keeps a date window and the four inputs needed for the earnings calculation. Neither route posts a journal. A saved conversion therefore cannot distinguish a confirmed zero from evidence the engine never kept.

## Select the capital evidence

Use the version 2 contract on both working-paper endpoints. Require an inclusive reporting period, an as-of date equal to the end, exact nonnegative drawings and contributions in Book currency, and one of two capital evidence modes.

- **Entered balances.** Require explicit signed opening and closing capital and a preparer-supplied reference for each figure. Preserve the stated amounts and reference text.
- **Statements of affairs.** Require two immutable version 2 statement revision IDs from [package 52](statement-of-affairs-revisions.md), in the same authorized Book and currency. Their as-of dates equal the day before the first period day and the final period day respectively. Use the stored capital from those exact revisions. Reject conflicting manually supplied capital fields and offer a separate estimate if the user wants to override the source. Store the IDs, dates and exact selected amounts, so a later revision cannot change a saved estimate.

Use the [working-paper precision rule](statement-of-affairs-revisions.md#save-a-dated-working-paper) for all entered monetary inputs. When using the conversion route, persist the nullable opening and closing debtor, creditor and cash inputs and the nullable receipts and payments totals. Accept exact nonnegative values for these supporting amounts. An explicit zero is known evidence. A null or omitted value means unknown. Where a statement is linked, keep any manually supplied supporting balances labelled as preparer-supplied. They do not replace statement amounts. Missing balances or receipts and payments amounts do not prevent estimates with complete capital and movement inputs, and the response identifies which supporting fields are unknown. The estimation route carries the same capital evidence and figures without requiring those auxiliary fields.

Apply `earnings_from_capital` unchanged after normalizing either evidence mode. Signed earnings are closing capital less opening capital plus drawings less contributions. Keep positive profit or positive loss as mutually exclusive magnitudes, with both zero for break-even. For opening 800, closing 760, drawings 50 and contributions 30, the result is a loss of 20. Cash balances and receipts totals are supporting data and never enter this formula. Save the selected basis, frozen inputs, outcome and authenticated preparer before returning.

## Compatibility and older papers

`create_conversion`, `list_conversions`, `create_estimation` and `list_estimations` in `rust/apps/api-server/src/accounting_book.rs` own the routes under both accounting prefixes. Keep the unversioned response shape and its monetary string types. On list requests, `calculation_view=2` selects the version-aware response. Unversioned conversion lists serialize unknown supporting amounts using the existing zero-string convention without changing what is stored. Version 2 requests and version-aware lists report those fields as strings or null, documented in the opt-in response schema. Current and historical paper versions can coexist in scoped lists. These estimates describe incomplete books alongside the normal posted financial reports. They neither create opening balances nor reclassify transactions.

Keep the old rows and IDs. Display their historical figures with version 1 metadata where the caller opts in to versioned reads. Mark each of the six legacy zero placeholders as unknown in that view, preserving the original value in the historical payload. The old receipts and payments zeros have ambiguous provenance and also require confirmation. A preparer can copy the paper into a new version after confirming evidence. Never replace an old zero with a reconstructed balance automatically.

For an inaccessible source say “Choose a statement of affairs you can access in this Book.” If a selected statement is dated incorrectly, say “The opening statement must be dated the day before this period, and the closing statement on its last day.” For mixed sources or missing manual references, say “Provide the two dated statement revisions or enter and document both capital balances.” A submitted supporting amount that is unreadable names its exact field and saves nothing.

## Verification plan

- Add `single_entry_zero_and_unknown_supporting_balances_round_trip` to the bookkeeping tests. Submit explicit zero, omitted values and a nonzero exact decimal together and read them back distinctly.
- In `rust/apps/api-server/tests/records_http.rs`, add `earnings_routes_agree_on_frozen_statement_revisions`. Compare both routes, revise the closing statement and verify the prior estimate remains tied to its original source revision. A statement ID from another Book must fail.
- Add `legacy_conversion_unknown_projection_preserves_original_payload` to the records HTTP tests. Compare both read contracts and check the journal and balance counts are unchanged.

Accept when both routes produce the same signed outcome for the same documented inputs and every reported supporting balance can be distinguished from an omission after readback.
