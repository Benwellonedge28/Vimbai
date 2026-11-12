# Duplication that is still here, and what removing it costs

The cheap duplication is gone: one rounding rule, one id stamp, one registration per
calculator, and `tests/test_no_duplicate_wiring.py` fails on the shapes that used to slip
through. What is left needs arithmetic to change, not code to move, so it is written down
with the shape of the fix rather than attempted blind.

## The cost-accounting pair

The same calculations exist twice, in two crates, with two arithmetics:

| calculation | exact, reachable over HTTP | float, reachable over the Python seam |
| --- | --- | --- |

Both sides are live, which is the problem. `/calc/<name>` calculators and the record-backed
endpoints run on exact `Money`; the PyO3 seam that Python services and the parity tests call
runs on `f64`. A rounding fix in one is not a fix in the other, and the two disagree on cents
at large amounts - the ledger-core `py_round` bug in commit 3b6b419c was the same disagreement
in miniature.

## Done as well: basis apportionment and process/product costing

Both were unreachable in the same way as CVP - nothing outside their own crate named them, and the
re-exports in two `lib.rs` files made them look used. `domains/costing` answers both: exact
largest-remainder apportionment across the seven bases, and one process/product costing used by both
services that used to answer each other's records.

## A dead public function is not a small problem

The float CVP was not a live second opinion, it was unreachable: nothing in the workspace called
`cost_accounting::cvp`. No seam function wrapped it and the API records module had always used the
exact crate. It was kept alive by its own re-exports in `lib.rs`, which is how a duplicate survives
a search - it looks imported, so it looks used. Deleted, re-exports and all; every one of the five
operations (contribution, quick analysis, target profit, multi-product mix, stored analyses) is
served by `domains/costing/src/cvp.rs`, with its own tests and the recorded parity capture.

The lesson from all three: a duplicate survives a search because it is *exported*. `pub use cvp::{...}`
made 426 lines of float arithmetic look like part of the crate's surface. The guard tests now fail on
the wiring shapes, but nothing yet fails on a module that is exported and unreachable - that is worth
adding.

## Done: job costing

Job costing was the smallest of the four and it is now one rule. `domains/costing/src/job_costing.rs`
owns `settle(total, contract)` - profit and margin - and both the record-backed endpoint and the
seam's `add_cost` call it. The seam still speaks `f64` because its request and response shapes do;
it converts, sums in `Money`, and converts back.

That found a third copy: the seam's unknown-cost-type branch added four `f64`s and applied its own
`max(1, contract_value)` margin. Same fix.

It also found a rule that was wrong and had been asserted by a test. Python divided by
`max(1, contract_value)`, so a contract of zero reported a margin of -5000% and a contract of 0.50
was measured against 1. A zero contract now reports no margin, and any real contract divides by
itself. No parity capture depended on the old figure.

## Why the rest cannot just be deleted

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
