# Business documents
Status: proposed extension of platform documents and invoicing.

## Catalogue
Quotes, estimates, orders, delivery notes, invoices, credit/debit notes, receipts, payment vouchers, statements, purchase requisitions/orders, goods received notes, expense claims, remittance advice, contracts and statutory reports. Each type declares issuer, recipient, required fields, lifecycle, numbering policy and whether it creates a posting intent. A quote alone never posts revenue.

## Contract
Document ID, Book, type/version, source record, template version, locale, currency, issue date, status, immutable issued snapshot and content hash. Draft editable; issued corrections use linked replacement or credit/debit note as applicable. Number allocation atomic per configured series; no silent reuse. Rendering PDF/print/export is separate from accounting.

## Integration
Extend existing rust/crates/platform documents owner; invoicing and other domains own business validity; application composes issue/post workflow; Flutter previews and shares. Templates data-driven and sanitized; attachments scoped and malware-scanned. Preserve existing invoice APIs.

## Acceptance
Concurrent numbering, locale and tax rendering, exact totals, duplicate issue, unauthorized download, offline draft reconciliation, immutable issued snapshot and accessible print output. Posting only via kernel; document renderer never computes independent financial totals.
