# Finance

The arithmetic a finance function asks for that is not a statement and not a cost: present value, returns, payback, break-even on an investment, and the decisions around insurance and personal finance.

Crate: `rust/crates/domains/finance`

## What it owns

1. Discounting: net present value over a stream of exact amounts, the internal rate of return, and the payback period.
2. Investment appraisal: the initial investment, the internal rate of return service's published addresses, and the net-present-value service's.
3. Insurance claims: the claim, what it covers, and the settlement arithmetic.
4. Personal finance: recurring items - a rent increase edits the existing item through PATCH rather than opening a second one, which is the behaviour the published address requires and a test now pins.

Every figure is `common::Money`. An IRR computed in floats is an IRR that disagrees with the ledger it is explaining.

## Services and addresses

| Service | Prefix |
| --- | --- |
| `continue-shutdown-decision-service` | `/continue-shutdown-decision` |
| `insurance-claims-service` | `/insurance-claims` |
| `internal-rate-return-service` | `/internal-rate-return` |
| `investment-monitoring-service` | `/investment-monitoring` |
| `net-present-value-service` | `/net-present-value` |
| `payback-period-service` | `/payback-period` |
| `personal-finance-service` | `/personal-finance` |
| `trade-finance-service` | `/trade-finance` |

## What it does not own

The statements. A net present value is an input to a decision; the balance sheet and the cash flow statement belong to `accounting.md`. Nor does it hold the investment itself - `assets.md` holds what the Book owns.

## Tests

Each calculator is unit-tested against a worked example, and the published addresses are walked over HTTP. The recurring-update path is covered by a test that raises a rent and proves the existing item changed rather than a fourth one appearing.
