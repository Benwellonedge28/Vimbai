# Work package 59 — Record one intangible amortization charge per period

Design baseline [ca0a15a4](https://github.com/Benwellonedge28/Vimbai/commit/ca0a15a467b3263bf2ae5a5342ee30ca2120252e).

## The current split write

`amortize` in `rust/crates/domains/assets/src/intangible_assets.rs` accepts nonempty period text, updates the asset, then inserts an `AMORTIZATION_ENTRY`. The code has no period uniqueness check. `monthly_charge` falls through to a fixed reducing-balance rate for a non-straight-line method, and the residual threshold is checked through a lossy comparison after accumulated amortization is increased.

## Charge against a verified basis

For an evidence-backed amortization policy, store the effective start month, method, positive useful life in months, cost, residual value, Book currency and revision. Straight line spreads depreciable cost over life. Reducing balance applies a configured annual percentage to the amount above residual, divided by twelve. Validate the rate within zero exclusive and 100 inclusive. For each method, follow the Book's posting-precision policy, validate that it fits the store and cap each charge at the exact remaining amount. In the final eligible month charge the remainder. A rounded zero before the final month is a valid period receipt with unchanged carrying value. Declare fully amortized only when accumulated amortization equals cost less residual exactly.

Accept a normalized `YYYY-MM` at or after the policy start. Require the next unrecorded month, with retries recognized before checking that sequence. Reserve `(Book, asset ID, month)` while locking the asset revision. In one scoped transaction insert the period receipt and update accumulated amortization, carrying value and status. The receipt records the policy revision, before and after values, the exact charge and authenticated actor. A repeated period request returns its stored answer even after the asset becomes fully amortized. Corrections require an attributed register adjustment with its own record. The amortization register never posts the journal itself.

## Routes and existing balances

Update `amortize_intangible` and `intangible_amortization` in `rust/apps/api-server/src/records.rs` on both intangible-assets prefixes. Continue to return the saved receipt fields and add policy and evidence metadata. Scope the receipt lookup to the asset before paging. During transition, require a valid normalized month on the new policy path. Only one charge writer may edit an asset with that policy. The stand-alone calculator in `rust/crates/domains/assets/src/amortization.rs` remains a preview and carries no receipt.

Read legacy balances and entries as originally recorded. Before assigning a policy to a legacy asset, verify cost less accumulated equals recorded carrying value, validate the residual boundary and review duplicate months and missing entries. Store the approved opening checkpoint and first eligible month. Unverified histories remain readable. For a gap say “Record or review the earlier amortization month first.” For an unresolved policy say “Set this asset’s amortization policy and verify its opening balance.” For a failed save say “The amortization charge was not saved. Retry this month.”

## Proof

- Add `last_intangible_charge_cannot_exceed_remaining_basis` in the module tests with positive residual and one remaining stored unit.
- Add `intangible_amortization_receipt_replays_after_full_amortization` to `rust/apps/api-server/tests/records_http.rs`. Retry through the records alias and refuse another Book's ID.
- Add `intangible_asset_and_month_receipt_are_atomic` to the provider cases in [package 47](subsidiary-journal-posting-receipts.md). Race the same month, inject a receipt write failure and verify exact readback after restart.

Finish when every verified carrying value is the initial reviewed basis less the sum of distinct monthly receipts.
