# Duplication that is still here, and what removing it costs

The cheap duplication is gone: one rounding rule, one id stamp, one registration per
calculator, and `tests/test_no_duplicate_wiring.py` fails on the shapes that used to slip
through. What is left needs arithmetic to change, not code to move, so it is written down
with the shape of the fix rather than attempted blind.

## The cost-accounting pair

The same calculations exist twice, in two crates, with two arithmetics:

| calculation | exact, reachable over HTTP | float, reachable over the Python seam |
| --- | --- | --- |
| CVP, break-even, margin of safety, operating leverage | `domains/costing/src/cvp.rs` (657 lines) | `calculations/cost_accounting/src/cvp.rs` (426 lines) |
| job costing | `domains/costing/src/job_costing.rs` (245) | `calculations/cost_accounting/src/job_costing.rs` (105) |
| basis apportionment | `domains/costing/src/basis_apportionment.rs` (377) | `calculations/ratios_statements/src/basis_apportionment.rs` (239) |
| process and product costing | `domains/costing/src/cost_calculations.rs` | `calculations/cost_accounting/src/process_product_costing.rs` |

Both sides are live, which is the problem. `/calc/<name>` calculators and the record-backed
endpoints run on exact `Money`; the PyO3 seam that Python services and the parity tests call
runs on `f64`. A rounding fix in one is not a fix in the other, and the two disagree on cents
at large amounts - the ledger-core `py_round` bug in commit 3b6b419c was the same disagreement
in miniature.

## Why it cannot just be deleted

The float side is what the Python parity captures were recorded against. Deleting it without
moving the seam would leave the Python tests asserting figures nothing produces any more, and
would quietly change API behaviour for every caller that has been reading floats.

## The order that works

1. Convert `cost_accounting`'s calculators to exact arithmetic one function at a time, keeping
   the same request and response shapes so the seam does not move.
2. Re-record the parity captures from the exact implementation, asserting the exact figures
   deliberately rather than mirroring the float drift.
3. Delete the float functions once the seam and the captures both point at the exact ones, and
   let `test_no_duplicate_wiring.py` prove the name is now defined once.

Until step 3, the two implementations stay, and this file is the reason - not an oversight.
