# Work package 54 — Derive control-account totals and running balances from the same lines

Working-paper design based on [ca0a15a4](https://github.com/Benwellonedge28/Vimbai/commit/ca0a15a467b3263bf2ae5a5342ee30ca2120252e).

## Two passes can disagree today

In `rust/crates/domains/accounting/src/bookkeeping.rs`, `create_control_account` recognizes an allowlist for the header totals, then uses another set of conditions to move the line balance. For instance a creditors sheet omits `interest_charged` from its header calculation even though that line increases the balance shown before the next row. The existing `running_balance` field represents the balance before each line. `list_control_accounts` filters by account type after reading a capped Book collection.

## Classify each line once

Add a typed classifier used by the opt-in `calculation_version: "2"` branch. The new working paper requires `account_type` equal to `debtors` or `creditors`, an inclusive period with an as-of date equal to its end, a signed opening balance in the account's normal orientation, and positive exact `Money` amounts in Book currency. Parse amounts through the shared [working-paper precision rule](statement-of-affairs-revisions.md#save-a-dated-working-paper). Rows carry unique opaque input IDs. Order by effective date and preserve submitted order for entries sharing a date. Store the canonical type, debit or credit side, exact amount and evidence reference on the saved row. Labels and references may be supplied by the Book.

| Sheet | Debit line | Credit line |
| --- | --- | --- |
| Debtors | Credit sale or interest charged to customer | Receipt, sales return, discount allowed, write-off or contra |
| Creditors | Payment, purchase return, discount received or contra | Credit purchase or interest charged by supplier |

The two headers describe sides. On a creditors sheet, a credit increases the normal balance and a debit decreases it. Define the wire types as `credit_sale`, `credit_purchase`, `interest_charged`, `payment_received`, `payment_made`, `sales_return`, `purchase_return`, `discount_allowed`, `discount_received`, `write_off` and `contra`. Accept each type only on the sheet where the table places it. A contra reduces both receivables and payables across their respective sheets. Require a cross-reference to the same Book's supporting evidence, and derive `is_contra` from the type. Refuse a supplied flag that disagrees. The sheet calculation records the selected side only and creates no entry on the other sheet.

For each line, preserve `running_balance` as the pre-line value and add `balance_after`. Sum debit and credit amounts in that same pass. Debtors close at opening plus debits less credits. Creditors close at opening plus credits less debits. An empty line array closes at opening. A negative normal balance is valid and indicates an opposite balance. Preserve account type, window, inputs and derived totals in the existing `CONTROL_ACCOUNT` kind. These figures describe the preparer's evidence and do not create a control account or change the ledger.

## Route behavior and existing sheets

`create_control` and `list_control` in `rust/apps/api-server/src/accounting_book.rs` serve the existing control-account routes. Add the versioned contract to their API documentation. Preserve historical type spelling and pre-line response semantics. Apply type filtering before cursor pagination, ordering equally dated rows by stable stored identity. Preserve the array shape for legacy list callers. Keep every legacy sheet and its results as saved, tagged as historical on read. A revised sheet links its earlier ID and requires the preparer to confirm any old discount label whose side does not match the new table. No backfill converts an old running balance into a guessed closing balance.

For an unsupported kind, say “This entry type cannot be used on this control account” and name the line ID. For missing contra evidence, say “Link the evidence for this offset before saving.” If an entry falls outside the period, explain which date the user must correct. A rejected line leaves the entire paper unsaved.

## Acceptance checks

- In the bookkeeping module, add `control_totals_and_balance_after_agree_for_every_supported_side`. Cover both sheets, include creditor interest followed by another line and confirm exact header and terminal line balances.
- Add `control_contra_requires_evidence_without_posting_an_offset` and `control_list_filters_before_its_cursor_limit` to `rust/apps/api-server/tests/records_http.rs`. Exercise both aliases, verify unchanged journal counts, and place matching sheets after the old read limit.
- Add `legacy_control_running_balance_remains_the_before_value` to the domain tests. Read a legacy record and ensure the new field appears only when its value is evidenced.

The implementation is ready when a reviewer can add each saved debit and credit once and match the reported closing balance on either type of sheet.
