# Work package 64 — Preserve rolling forecast periods through concurrent updates

Forecast source review [ca0a15a4](https://github.com/Benwellonedge28/Vimbai/commit/ca0a15a467b3263bf2ae5a5342ee30ca2120252e).

## Baseline write behavior

`roll` and `update_actual` in `rust/crates/domains/budgeting/src/rolling_forecast.rs` each read and replace the embedded periods of a `ROLLING_FORECAST`. Roll appends periods without validating duplicates. Updating an unknown period returns a null variance after an update call. Missing actuals can reach the existing parser's zero default. Concurrent commands can replace one another's changes.

## Use frequency and revision to identify a period

Give a verified forecast an immutable currency, frequency and calendar basis. Use a versioned Book calendar to resolve weekly, monthly and quarterly labels into continuous dated intervals. For a Book using the calendar-year convention, ISO week-year and week, calendar month and calendar quarter are the corresponding formats. Reject calendar changes on an existing verified forecast. A new forecast can adopt the new calendar. Persist the normalized period key and inclusive date boundaries. The intended horizon grows from its latest dated period. Append one or more consecutive periods in order, rejecting duplicates, gaps and rows prior to the frontier. Each new row holds its baseline and its assumptions version. Current period is derived from the Book date and boundaries, with null when outside the horizon. Completing a period does not erase its baseline.

An actual update names an existing period, exact amount and dated source reference. An actual of zero is valid. A correction names and supersedes a previous observation with an authenticated actor and reason. Preserve the previous observation and calculate the new variance as actual less baseline. Use absolute baseline for the percentage denominator and publish null when it is zero. A Book forecast is planning evidence and never writes ledger actuals.

Reserve a Book-and-forecast-scoped operation key and apply each write with the caller's expected revision. Persist an operation receipt and new forecast version together. Return the prior result for a matching key before comparing revisions. Report “This forecast changed while you were editing it. Reload and reapply the change” for a stale revision. For an unknown period say “This period is outside the forecast. Add it before recording an actual.” A period parse refusal names the first invalid row.

## Route and migration details

Have `open_rolling_forecast`, `record_forecast_actual`, `roll_forecast` and `one_rolling_forecast` in `rust/apps/api-server/src/records.rs` carry the evidence version, revision and key for verified requests. Preserve legacy routes and response fields, refuse unguarded writes against a verified row, and register the new payload contract against the existing rolling-forecast routes. Implement version and operation reservation with a shared store transaction such as [package 22](opening-balance-receipts.md).

Keep IDs and original periods for older forecasts. Normalize uniquely parseable keys, but surface duplicated dates, conflicting actuals and gaps for review before enabling the new writer. A migration approval stores the complete original record and reasons for chosen corrections. Do not infer an actual from a missing field.

## Tests

- Add `forecast_roll_refuses_duplicates_and_gaps_across_year_boundary` to the budgeting unit tests. Exercise weekly, monthly and quarterly normalization.
- Add `forecast_update_unknown_period_does_not_mutate_state` to `rust/apps/api-server/tests/records_http.rs`, covering a valid zero next to an unavailable percentage.
- Add `concurrent_forecast_roll_and_actual_cannot_lose_each_other` to the shared provider suite in [package 47](subsidiary-journal-posting-receipts.md). Refresh the losing revision, retry the operation, and then replay both keys after restart.

Accept when concurrent appends and observations retain all committed information and historical periods still expose their original baseline.
