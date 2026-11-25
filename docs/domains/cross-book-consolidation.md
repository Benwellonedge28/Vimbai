# Consolidate authorised Books with evidence for every adjustment

Source baseline `2df3a7685b1109756fad0cca4d7b648b25790bdb`. Depends on 03, 10 and 17. First delivery covers one parent, one subsidiary and one reporting period with a verified example.

## Existing entry points

`rust/crates/calculations/group_accounting/src/consolidation_views.rs` exposes elimination, non-controlling interest, goodwill and currency translation. Its current elimination calculation accumulates floating-point amounts from the request. `rust/crates/calculations/treasury_fx/src/fx_translation.rs::translate_financials` covers translation calculations. Read actual Book balances through the accounting ledger layer and Book access through `rust/crates/platform/tenancy/src/access.rs::allows`. Treat the standalone calculators as separately published compatibility surfaces until their callers are identified.

## Run and review

Create an authorised consolidation application service. Version the group membership with ownership shares, effective dates, control decisions and account mappings. Each run pins the reporting window, member Book IDs, immutable trial-balance snapshots, mapping versions, selected reporting framework and FX rate IDs. Authorise the caller against every included Book before reading. Store exact decimal working amounts throughout. Require coverage for every input account or list an explicit, reviewed exclusion. Produce translation as an adjustment view under ADR-012.

Pair intercompany receivable/payable and revenue/expense items by approved linkage, compatible period and common currency. Carry both source posting IDs. Leave unmatched movement as an exception, with no automatic balancing plug. Store manual adjustments as balanced, attributed entries in the consolidation working paper; keep subsidiary ledgers untouched. Derive ownership adjustments from the approved profile and preserve effective dates. The report presents original mapped totals, translations, eliminations and final balances. Run approval names the exact version and reviewer. Re-run after a material changed input, with the earlier revision still readable.

## Legacy requests and errors

Preserve the calculator wire outputs until the versioned Book-backed contract ships. Do not silently transfer caller-provided calculator payloads into approved group history. Missing member access refuses the run before disclosing its balances. Missing mappings, periods, unsupported control cases or unresolved elimination differences block approval and identify which item requires review.

## Acceptance tests

- `consolidation_refuses_a_book_the_reviewer_cannot_open` checks every member and the saved report read path.
- `matched_elimination_is_balanced_and_preserves_source_ledgers` verifies posting IDs and totals.
- `high_precision_amounts_survive_translation_and_elimination` compares an independent decimal worksheet.
- `changing_a_source_snapshot_requires_a_new_approval` proves an old report is reproducible.
- Done when an independent reviewer reconciles the approved parent-and-subsidiary fixture from source ledgers to group statements and every unresolved difference remains visible.
