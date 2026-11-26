# Work package 78 — Freeze an issued invoice and revise drafts coherently

Invoice lifecycle reviewed at [5c2d0018](https://github.com/Benwellonedge28/Vimbai/commit/5c2d0018ed10d212ab43eda94a0a8d2509c3ea0d).

## Baseline behavior

`update_invoice` in `rust/crates/domains/invoicing/src/invoices.rs` can replace item rows without checking the total unless the total was also supplied. It recomputes `amount_due` only when the discount appears in the patch. It also accepts a status or due date independently of the other fields. `delete_invoice` removes the invoice without deriving its lifecycle state.

## Apply the full draft revision

On an opt-in draft edit, read the expected revision, overlay permitted fields and validate the whole result. Require a valid invoice and due date and require due date on or after invoice date. Derive lines and gross using exact `Money` and the invoice's pricing contract, then apply validated discounts once. Supplied control totals must agree at the declared currency precision. Preserve the existing legacy tolerance as a historical input convention. Compose a complete new revision and replace the draft pointer with compare-and-swap. A race with another draft edit or issue returns a conflict without partially applying fields.

Issue is a separate action that freezes customer identity, document currency, dated line evidence, net amount and revision. An issued invoice remains readable with its original number and amounts. Keep a draft cancellation as an audited state. Correct an issued document through a linked replacement or credit document under existing accounting rules. A change to a draft or display record cannot modify posted journals. Payment-derived fields are owned by the [allocation contract](invoice-payment-allocation-receipts.md).

## Compatibility and failures

Make `put_invoice`, `post_invoice`, and `delete_invoice` in `rust/apps/api-server/src/records.rs` select the appropriate path for versioned records. Reject unguarded writes on those records and preserve current routes. Existing invoices remain dated historical records. Validate their stored line and total agreement before allowing migration to a draft. An invoice already issued or carrying payment evidence is preserved at its recorded revision.

For stale edits show “This invoice changed. Open the current revision.” For invalid dates say “The due date must be on or after the invoice date.” For issued edits say “This invoice was issued. Record a correction against it.”

## Verification

- Add `invoice_draft_line_only_patch_recomputes_gross_and_net` and `invalid_draft_due_date_leaves_prior_revision_readable` in the invoice module.
- In `rust/apps/api-server/tests/records_http.rs`, add `issued_invoice_rejects_mutation_or_delete_and_retains_number`.
- Exercise `invoice_issue_and_edit_cannot_commit_against_same_revision` on both providers in [package 47](subsidiary-journal-posting-receipts.md).

Accept when every versioned draft reads back a coherent total and issue freezes exactly the validated version.
