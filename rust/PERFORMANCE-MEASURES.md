# vimbai_performance_measures

Six stateless services in one crate, safe Rust, no database and no clock:

| module | service |
| --- | --- |
| `scorecard` | balanced-scorecard |
| `controls` | internal-controls-testing |
| `profit` | profit |
| `inventory` | inventory-valuation |
| `realizable` | net-realizable-value |
| `fx` | foreign-exchange |

Parity figures were captured by running each service on canonical inputs.

## Defects preserved, each pinned by a test

*Scorecard scale.* A perspective score is weighted achievement divided by total
weight, then multiplied by 100 again, so one KPI at 80% scores 8,000. The overall
score is the mean of four such numbers, which means the 90-point "excellent"
threshold is cleared by a card whose only KPI reached 20% of target, and the 70
point warning can never fire.

*Hedge loss scale.* The rate gap is converted to a percentage and then multiplied
by the unhedged amount, so a 3.26% move on 100,000 reports 326,086.96 instead of
3,260.87.

*Scrap value.* Without sale proceeds the scrap value is zero, even when a disposal
cost was incurred.

*Inventory.* LIFO consumes purchases before the opening layer while FIFO consumes
opening stock first, and neither checks for an oversell, so a short position
produces a negative ending quantity silently.

*Controls.* A significant deficiency is counted as both significant and deficient,
so the two columns overlap. An untested control with a zero sample counts as
effective.

Both scale errors are decisions, not slips: correcting them moves reported scores
and hedge figures that clients have already seen.

## Not ported yet

`/profit-compare` and `/net-profit`, and `/compare-cost-nrv`, are not in the crate.
Their logic was not read before the port and guessing an endpoint's contract is not
acceptable, so they stay on the Python service until the next pass reads them.
