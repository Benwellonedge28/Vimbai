# Work package 80 — Retain the discount rules that priced an invoice

Discount source reviewed at [5c2d0018](https://github.com/Benwellonedge28/Vimbai/commit/5c2d0018ed10d212ab43eda94a0a8d2509c3ea0d).

## Pricing trace

`quote_discounts` and `apply_for` in `rust/crates/domains/invoicing/src/discounts.rs` are separate callers of rule selection over stored `DISCOUNT_RULE` rows. One previews and the other applies rules during invoice creation. Rule selection currently reads a capped collection. Keep this module as pricing owner and make the applied rule evidence explicit.

## Resolve a versioned quote

Each verified rule has stable identity, immutable revision, effective dates, exact amount or rate, eligibility and application order. On an edit append a revision and preserve the previous one. Resolve requested IDs under Book scope before paging or matching, and require active eligibility for the invoice date. Use the existing discount arithmetic and ordering rules, with a stable ID tie-break when two otherwise equal eligible rules compete. Freeze the input item snapshot, ordered rule revisions, exact reductions and total in a versioned quote. Reject an adjustment that exceeds the exact eligible basis. Expose a clear no-discount result for an empty eligible set.

`post_invoice_discount_quote` and `post_invoice` in `rust/apps/api-server/src/records.rs` should carry a scoped quote ID and expected draft revision. The versioned issue operation verifies that the item snapshot and eligible revisions still agree. If either changes, request a new quote before freezing the invoice. Persist the quote evidence with the issued invoice and keep it readable after later rule revisions. The quote itself never posts or changes an invoice. Keep the unversioned request shape under ADR-004 and attach revision evidence only where verified.

## Historical rules and errors

Preserve saved rule IDs and historical discount amounts. Treat an old applied amount without recoverable rule inputs as legacy, and never infer the rule from a coincidentally equal discount. Reject an unavailable rule with “One selected discount is unavailable for this Book and date.” A stale quote says “This invoice or its discounts changed. Review a new quote.”

## Verifiable outcomes

- Add `discount_quote_names_exact_rules_and_survives_later_rule_edit` in the discount tests.
- Add `issue_refuses_stale_discount_quote_without_writing_invoice` to `rust/apps/api-server/tests/records_http.rs`, varying the line amount and effective day separately.
- Add `discount_lookup_can_resolve_requested_rules_past_legacy_cap` in the same HTTP suite and confirm a foreign rule ID has the same response as missing.

Accept when a reviewer can reconstruct the discount of an issued invoice from its saved rule revisions without querying the current discount register.
