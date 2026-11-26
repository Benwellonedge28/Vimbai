# Work package 58 — Make fixed-asset schedules dated, reproducible reads

Source review [ca0a15a4](https://github.com/Benwellonedge28/Vimbai/commit/ca0a15a467b3263bf2ae5a5342ee30ca2120252e).

## Current schedule behavior

`generate_schedule` in `rust/crates/domains/assets/src/asset_schedule.rs` stores totals from a capped read of current `FIXED_ASSET` rows. The supplied date becomes a label. The same function answers `GET /fixed-assets-schedule/schedule/latest` via `asset_schedule` in `rust/apps/api-server/src/records.rs`, writing a new schedule every time. The schedule also shares the register's partial update path. `update_asset` passes a broad patch to the shared record, so historical valuation cannot be inferred safely from its current fields.

## Compute a view with a stated evidence basis

Introduce `GET /fixed-assets-schedule/schedule/as-of?date=YYYY-MM-DD` and its records alias. Build the date view from a Book-scoped consistent read over acquisition, depreciation and disposal evidence. Include assets acquired by the target day and held at the end of that day, using the lifecycle in [package 57](fixed-asset-disposal-evidence.md). Use depreciation effective dates through the target day inclusive. For the month-level register charges, the effective date is the last day of the recorded month. Include the acquisition day and exclude an asset on its evidenced disposal day. Reconstruct from a verified opening snapshot plus subsequent events when earlier history is unavailable. A current row is sufficient for a historical projection only if its relevant opening facts and subsequent changes can be evidenced.

Return exact Book-currency amounts and a common source revision, with stable asset IDs and category totals computed from the same cohort. Scope before paging. Use a cursor bound to the requested day, ordering and source revision, and force restart if that revision cannot be read on a later page. Keep totals over the entire verified cohort even on later pages. If any relevant asset lacks trustworthy evidence, return its scoped ID and a reason with status `incomplete`. The verified totals are null until every relevant asset is accounted for. The read does not write a schedule row or change closed periods.

## Save a reviewed schedule explicitly

Keep `POST /schedule/generate` for legacy callers and add an evidence-backed option that persists the calculation version, frozen input revision, exact figures, missing-evidence state and ordered asset revisions under `ASSET_SCHEDULE`. Only a complete view can become a verified saved schedule. Use a Book-scoped lookup to expose that exact saved version by ID. A separate read of the most recent verified snapshot can order by persisted creation time with ID as the tie-breaker. Keep the legacy latest path during deprecation, then move it to the read-only contract in accordance with ADR-004. In the verified flow, financial fields cannot be edited through a generic patch. Reference verified, dated corrections instead.

Existing schedules retain their dates and totals and are identified as legacy snapshots of the register. If an asset's history cannot support a requested date, show “This asset’s value for that date could not be verified. Review its register history.” An invalid date says “Enter a valid schedule date.”

## Tests

- In `asset_schedule.rs`, add `as_of_schedule_honors_acquisition_depreciation_and_disposal_cutoffs` with a fully depreciated asset still held at salvage.
- In `rust/apps/api-server/tests/assets_register_http.rs`, add `as_of_schedule_reads_without_saving_and_pages_past_the_old_cap` and `incomplete_schedule_does_not_present_partial_totals_as_verified`.
- Add `saved_schedule_replays_original_totals_after_later_asset_change` to the same suite, including a foreign Book ID refusal.

Done when the same retained evidence answers the same historical date, and repeatedly opening a schedule writes no rows.
