# Work package 77 — Keep invoicing customers identifiable after archival

Customer-register baseline [5c2d0018](https://github.com/Benwellonedge28/Vimbai/commit/5c2d0018ed10d212ab43eda94a0a8d2509c3ea0d).

## What can happen today

`create_invoice_customer` and `invoice_customer` in `rust/crates/domains/invoicing/src/invoices.rs` search a capped customer list for the caller's ID. A customer beyond that limit can be missed during creation and lookup. `delete_invoice_customer` deletes the row even though invoices store the public customer identifier.

## Reserve identities in the Book

Retain the exact case-sensitive public identifier on old rows. For new customers validate the identifier as nonblank and reject surrounding whitespace. Reserve it permanently within the authorized Book, using a unique store key with the ID's exact bytes. Insert customer and reservation in a single operation. Resolve reads through that key before applying list bounds. Store the customer row's immutable internal ID on subsequently issued invoices and retain the display reference that was used at issue. Customer renames change the address-book profile. An issued invoice retains the billed details captured by its issue revision.

Replace deletion with archival for the versioned contract. Require the expected profile revision and record the authenticated actor and reason. Active lists omit archived customers, while direct historical invoice references can still resolve them. A draft for an archived customer cannot be issued until the customer is restored or replaced. Serializing archive and invoice issue prevents an issuance from silently succeeding against an archived profile. Archival, restoration and contact edits do not alter ledger entries or invoice dates.

## Consumer rollout

Update the customer adapters `post_invoice_customer`, `get_invoice_customer`, `put_invoice_customer`, and `delete_invoice_customer` in `rust/apps/api-server/src/records.rs`. Preserve the public lookup key and success fields. Keep legacy reads usable under ADR-004. Project archived identity metadata only to callers with Book access. Rebuild reservations from the full scoped register. Conflicting rows preserve their internal IDs and require explicit historical invoice mapping before the public ID can be used again.

Say “This customer ID is already reserved in this Book” for duplication, “Restore this customer or choose another before issuing the invoice” for an archived customer and “This customer changed. Reload its details” on a stale revision.

## Tests and completion

- Add `customer_lookup_and_reservation_reach_beyond_legacy_cap` to `rust/apps/api-server/tests/records_http.rs`.
- Add `archived_customer_still_resolves_original_invoice` beside it, including another Book with the same public ID.
- Add `concurrent_customer_create_cannot_reserve_one_public_id_twice` to the provider target in [package 47](subsidiary-journal-posting-receipts.md).

Finish when an invoice can reopen its customer identity after archival and no Book can reuse it for a different customer.
