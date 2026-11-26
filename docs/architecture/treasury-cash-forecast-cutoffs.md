# Work package 73 — Forecast cash from an evidenced opening position and dated movements

Treasury-management baseline [3a2c2ca1](https://github.com/Benwellonedge28/Vimbai/commit/3a2c2ca16978df88a95ad6b1c1decb2148a8707b).

## Observed calculation

`cash_forecast` in `rust/crates/domains/treasury/src/treasury_management.rs` sums the returned movements without excluding rows after the query's current date. It uses zero for the forecast opening balance when there is no saved position, even though `get_cash_position` derives a balance from movements. The no-history branch also returns zero ending position, and the response leaves the end date blank. `stored_position` sorts candidate rows by ascending creation label before choosing the first.

## Define the date window and basis

Offer an opt-in query with the company, opening date, observation-window start, and positive forecast duration within the Book's configured forecast horizon. Expose the allowed duration in the versioned response and reject a request beyond it before reading movements. Validate all dates in the Book's calendar. Treat the observation interval as inclusive from its start through the opening date, and the projected interval as the following day through the requested number of days inclusive. Read a consistent Book-and-company snapshot of dated, classified movements in one specified currency. Future records do not contribute to the average. An unreadable date or amount makes the estimate incomplete rather than decreasing it silently.

Choose the latest verified available-cash position effective on or before the opening date. Roll that position forward using observed movements after its date through opening day. A derived opening is allowed only from a verified starting balance and a complete movement window. Where no position can be reconstructed, return null for opening and ending and explain the missing evidence. Calculate inflow and outflow separately from exact sums over the full inclusive observation-day count. Multiply by the forecast duration and round once at publication. Report whether the observed history is empty. Return null projections when there is no evidence for a rate, while retaining an independently verified opening position. The answer carries source cutoff, balance source and window dates.

## Routes and older records

Update `company_cash_forecast`, `company_cash_position` and `set_company_cash_position` in `rust/apps/api-server/src/records.rs` and the corresponding domain readers. Preserve the legacy response fields for callers that do not request the evidence contract. Existing cash positions and flows remain readable. Validate dated amounts and currency before admitting a historical row to the verified basis. A stale observation remains separate from newer evidence. Saving or reading a forecast never changes treasury balances or ledger postings.

For a missing position say “Verify the available cash at the opening date to calculate an ending balance.” For currency ambiguity say “Choose a currency and review movements recorded without one.” A date window error names the start or duration that must change.

## Required tests

- In the treasury-management tests add `cash_forecast_excludes_future_flows_and_counts_both_window_endpoints`.
- Add `cash_forecast_preserves_verified_opening_when_history_is_empty` beside that case.
- In `rust/apps/api-server/tests/records_http.rs` add `forecast_and_position_share_the_same_dated_opening_evidence`, covering multiple observations and a Book boundary.

Complete when identical as-of inputs reproduce the same forecast and a future movement cannot change an earlier estimate.
