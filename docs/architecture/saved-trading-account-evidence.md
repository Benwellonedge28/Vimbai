# Work package 38 — Preserve the calculation evidence for saved trading accounts

Proposed design against `f9795238b56a8a02c8c7513cc72e484f69ff9b2d`.

## Existing path
`rust/crates/domains/accounting/src/trading_account.rs::generate` invokes `compute` and stores totals plus `details_json`. `result_of` substitutes an empty object when its detail field fails to decode. The generate, list and get adapters in `rust/apps/api-server/src/records.rs` consume those results.

## Evidence contract
Persist a version and a canonical snapshot of the exact inputs used by `compute`, using Money strings and the Book's scope. Store input labels as supplied and record the effective period. Retain display details and the existing total fields. Write the receipt once per successful generation as a single record. A fresh generation produces a new snapshot and does not alter a previous one. This receipt identifies the inputs to a calculation. It does not assert that the figures were sourced from the ledger.

On get or list, verify version, decode the input snapshot, recompute through the same version of the formula, and compare the stored totals. Expose `evidence_status` as `verified`, `legacy` or `needs_review`. A corrupt record remains visible with its ID and saved totals, plus “The saved calculation details could not be verified.” Keep a failure in one receipt from hiding otherwise readable list results. Reject malformed new input before creation. Ledger rows and Book balances are unaffected.

## Upgrade and acceptance
Treat rows without a version as legacy. Retain the old details and totals, including incomplete evidence. Never tag legacy totals verified by guessing inputs. Keep the current response fields while adding the evidence status and snapshot version.

- `saved_trading_account_recomputes_from_its_original_inputs` verifies the receipt after reopening the store.
- `corrupt_trading_account_details_remain_visible_for_review` tests malformed JSON and totals mismatches.
- `legacy_trading_account_stays_readable_without_claiming_provenance` exercises both scoped get and list.

Done when a saved row reproduced on either store yields the exact original totals, and another Book cannot read its evidence by ID.
