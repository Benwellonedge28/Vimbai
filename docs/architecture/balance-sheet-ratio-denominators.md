# Work package 39 — Explain undefined ratios in stored balance sheets

Proposed design against `f9795238b56a8a02c8c7513cc72e484f69ff9b2d`.

## Existing path
`rust/crates/domains/accounting/src/balance_sheet.rs::ratio` substitutes Money one for divisors below one. `ratios` uses the stored balance sheet. This causes valid positive fractions below one and undefined ratios to be reported using the same substitute divisor.

## Calculation contract
For each ratio in `ratios`, define its numerator and denominator from the current field mapping. Divide by the exact positive denominator, even below one. A zero denominator produces an unavailable result with “This ratio is unavailable because its denominator is zero.” A negative denominator produces an unavailable result with a separate explanation that the denominator is negative. Leave the assets, liabilities and equity values unchanged. Keep ratio arithmetic in Money and use the existing presentation precision. Keep valid zero numerators as calculated zero when the denominator permits division.

Add per-ratio evidence carrying the denominator and reason when unavailable. Preserve names and successful value encoding. Consumers that assume the legacy fields are always numeric must be updated alongside the response before returning unavailable values. The implementation must locate all route and client usages of this stored-sheet ratio contract before enabling the change. Do not apply the rule to unrelated ledger-derived reports automatically.

## Stored data and verification
No migration is needed for saved sheets because `ratios` computes from stored inputs. Existing corrupt Money values must still produce the existing validation error.

- `balance_sheet_ratio_uses_positive_fractional_denominator` proves 2 divided by 0.5 yields 4.
- `balance_sheet_ratios_distinguish_zero_and_negative_denominators` checks both messages and a neighbouring valid ratio.
- `reading_ratios_does_not_modify_the_balance_sheet` confirms an existing record is byte-identical after read.

Done when every ratio can be traced to its real denominator and the response contract tests cover valid and unavailable states.
