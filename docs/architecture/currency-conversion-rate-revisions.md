# Work package 70 — Resolve and retain the exchange rates used for a dated conversion

Reviewed against [3a2c2ca1](https://github.com/Benwellonedge28/Vimbai/commit/3a2c2ca16978df88a95ad6b1c1decb2148a8707b).

## Existing mismatch

`convert_currency` in `rust/crates/domains/banking/src/currency.rs` loads `latest_rates` and passes the request's rate date to `conversion_result` for display. `update_exchange_rate` edits the selected row in place. The separate `rate_as_at` already delegates dated lookup to `RateTable::in_force` in `rust/crates/common/src/fx.rs`. This work package covers currency-service previews and rate provenance. Document-period revaluation follows [its own posting design](fx-revaluation-period-evidence.md).

## Preserve the basis

For an opt-in dated conversion require a valid calendar date, enabled Book currencies, exact amount and declared rounding. Resolve only eligible rates effective on or before that day from the authorized Book. Use the shared date and source selection semantics in `RateTable::in_force`. Direct rates precede an explicitly allowed inverse. If a Book configures a triangulation currency, use two eligible dated legs, keeping both row IDs and their individual effective dates. Rate revisions store immutable rate, source and effective date plus optional superseded ID and reason. Resolve overlapping same-priority observations by a single explicit applicable revision. If ambiguous, ask for evidence rather than relying on collection order. Identity conversion uses exact one and requires no rate row.

Expose input, exact selected rate or legs, source IDs, intermediate precision and rounded result. Resist unsupported rounding rather than echoing its name. Round once in the target currency according to its declared precision. Maintain a replay read by revision ID for saved source rows. A new correction cannot alter the answer reconstructed from an earlier set of IDs.

## Integration and old evidence

Extend `convert_currency`, `convert_batch`, `create_exchange_rate` and `update_exchange_rate`. `rust/apps/api-server/src/records.rs` should route opt-in dated previews and revision writes to the new contract under its currency routes. Preserve the legacy wire shape. Reject in-place edits to verified revisions and keep earlier IDs readable. Legacy rows may serve dated conversions after review. A historic response without saved rate IDs can retain its numbers as historical evidence, but its precise source cannot be inferred after an edit. Default sample rates are excluded from verified results unless a Book explicitly adopts sourced observations. No preview posts an exchange difference.

Say “No verified rate is available for this pair on the requested date” when a leg is missing and “More than one rate could apply. Select the source to use” when eligible revisions conflict.

## Acceptance

- Add `dated_conversion_ignores_future_rows_and_returns_actual_rate_date` to `rust/apps/api-server/tests/currency_http.rs`.
- In the currency module, add `dated_triangulation_retains_both_legs_and_rounds_once`, including an inverse and a currency with no fractional units.
- Add `corrected_rate_does_not_change_an_older_revision_replay` to the currency HTTP suite, with an inaccessible revision ID and mixed-success batch.

Done when every verified conversion can be reproduced from the source revisions it names.
