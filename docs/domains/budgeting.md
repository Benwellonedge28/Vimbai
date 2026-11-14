# Budgeting

What the Book intends to do with its money, and the arithmetic that turns an intention into something a person can be held to: budgets, forecasts, scenarios, sensitivities and the appropriations that spend against them.

Crate: `rust/crates/domains/budgeting`

## What it owns

1. Zero-based budgeting: every line justified from zero rather than from last year, which is a different arithmetic and not merely a different presentation.
2. Activity-based budgets: the activities a cost is driven by, and the cost driver rates that turn activity into money.
3. Rolling forecasts, which move the horizon rather than re-planning the year.
4. Scenario modelling and analysis: several futures side by side over the same period, and the comparison between them.
5. Sensitivity analysis: how much a result moves when one assumption does, with the assumption stated rather than implied.
6. Appropriation control: what may be spent against an appropriation, what has been, and the refusal when the two disagree.

A budget figure is exact money. A forecast is still money, and is never a float that happened to round well.

## Services and addresses

| Service | Prefix |
| --- | --- |
| `activity-based-budget-service` | `/activity-based-budget` |
| `appropriation-control-service` | `/appropriation-control` |
| `rolling-forecast-service` | `/rolling-forecast` |
| `scenario-analysis-service` | `/scenario-analysis` |
| `scenario-modeling-service` | `/scenario-modeling` |
| `sensitivity-analysis-service` | `/sensitivity-analysis` |
| `zero-based-budgeting-service` | `/zero-based-budgeting` |

## What it does not own

The actual. This crate says what was intended; the ledger says what happened, and the variance between them is a reporting question. Nor does it post: an appropriation approval is a decision, not an entry.

## Tests

Each module carries its own arithmetic tests, including the appropriation refusal at the limit, the activity-based driver rates, and the scenario comparison over a shared period. The published addresses are walked by the HTTP suite so a client's call is proven against the same arithmetic.
