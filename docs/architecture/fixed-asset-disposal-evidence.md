# Work package 57 — Distinguish fully depreciated assets from disposed assets

Proposed against [ca0a15a4](https://github.com/Benwellonedge28/Vimbai/commit/ca0a15a467b3263bf2ae5a5342ee30ca2120252e).

## Register behavior at the baseline

In `rust/crates/domains/assets/src/fixed_assets_register.rs`, `depreciate` marks an asset disposed when book value reaches salvage. `summary` excludes it from holdings. `dispose` refuses any such asset before checking for disposal evidence. For an active asset, it updates the asset before validating the required disposal date and inserting `ASSET_DISPOSAL`. Failure can therefore leave a disposed register row without a disposal record. This is the register service. The [accounting depreciation posting design](depreciation-posting-recovery.md) covers the separate journal flow.

## States and writes

Use exact `Money` comparison at the salvage boundary. Set `fully_depreciated` when the remaining depreciable amount reaches zero. Both active and fully depreciated assets remain held and contribute cost, accumulated depreciation and net book value to the register summary. A fully depreciated asset accepts disposal, while another depreciation request returns “This asset has reached its residual value.” Keep the existing held and disposed counts usable and add the fully depreciated count as a subset of held assets.

In `dispose`, parse a calendar date on or after acquisition, validate an explicit nonnegative Book-currency proceeds amount and a supported disposal method, then read the current carrying value. Allow no proceeds for a recorded retirement. Within a scoped transaction reserve the asset ID as the single disposal identity, insert the receipt and update the register state together. Use the transaction boundary proposed in [package 22](opening-balance-receipts.md), with the same asset revision guarding concurrent depreciation. Persist effective date, method, proceeds, cost, accumulated depreciation, net book value, gain or loss, notes and authenticated actor. Gain or loss is proceeds less net book value. A matching retry returns the original receipt. A changed submission says “A disposal has already been recorded for this asset. Open the original record.” A failed transaction leaves the asset held and says “The disposal could not be saved. Retry the same details.” The receipt concerns the asset register and contains no journal posting or transfer instruction.

## Integration and historical rows

Update `dispose_fixed_asset`, `depreciate_asset` and `fixed_asset_summary` in `rust/apps/api-server/src/records.rs` under both register prefixes. Keep supported existing disposal inputs during transition. Check `fixed_assets_service.dart`, `fixed_asset_register.dart` and `fixed_asset_page.dart` under `apps/flutter/lib/features/assets/` for serialization, status and action handling. Show fully depreciated assets as held and keep the disposal action available.

Review historical disposed rows using the complete Book-and-asset-scoped disposal and depreciation history. A matching disposal receipt preserves the disposed state. Reclassify as fully depreciated only after an authorized review confirms the asset is held and ties the last depreciation to salvage. Missing histories stay marked for review. Keep all original record IDs and prior amounts.

## Acceptance

- Add `asset_at_salvage_is_held_and_can_be_disposed` to the register unit tests. Include a nonzero salvage amount one stored unit away from the prior value.
- Add `register_disposal_invalid_date_leaves_asset_held` to `rust/apps/api-server/tests/assets_register_http.rs` and cover both route aliases.
- Add `register_disposal_receipt_and_state_commit_together` to the provider integration target in [package 47](subsidiary-journal-posting-receipts.md). Exercise failure between writes, competing disposals and replay on SQLite and PostgreSQL.

Accept when the register reconciles held assets and can open one dated receipt for each newly recorded disposal.
