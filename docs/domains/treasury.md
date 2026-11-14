# Treasury

The Book's cash, its debt, and the risks around both: how much cash it needs, how much it has, what it costs to borrow, and what happens to the answers when a rate moves.

Crate: `rust/crates/domains/treasury`

## What it owns

1. Cash management and cash optimisation: the position, the shortfall, and the movement that closes it.
2. Debt management: facilities, drawdowns, and the cost of each.
3. Foreign exchange: the exposure and the rate that translates it.
4. Treasury analytics: cash adequacy in days, debt service as a share of inflow, and a runway that does not pretend to be infinite. The figures the retired Python service answered are pinned exactly: 500000 of cash against 150000 of monthly burn is 100.000000 days; 30000 of short-term debt against 200000 of inflow is 15.000000 per cent; no burn floors the runway at 999 days; a net receivable of -50 scores as good. That crate was the last parity oracle and its numbers are now a fixture rather than a service.
5. Treasury policy, treasury risk, treasury compliance and treasury reporting: what the Book is allowed to do with its cash, what could go wrong, and whether it did.
6. Sovereign treasury, for a treasury operated at that scale, and petty cash, which answers on its own prefix.

## Services and addresses

| Service | Prefix |
| --- | --- |
| `cash-management-service` | `/cash-management` |
| `cash-optimization-service` | `/cash-optimization` |
| `debt-management-service` | `/debt-management` |
| `foreign-exchange-service` | `/foreign-exchange` |
| `petty-cash-service` | `/petty-cash` |
| `sovereign-treasury-service` | `/sovereign-treasury` |
| `treasury-analytics-service` | `/treasury-analytics` |
| `treasury-compliance-service` | `/treasury-compliance` |
| `treasury-management-service` | `/treasury-management` |
| `treasury-policy-service` | `/treasury-policy` |
| `treasury-reporting-service` | `/treasury-reporting` |
| `treasury-risk-service` | `/treasury-risk` |

## What it does not own

Moving the money. A treasury decision is recorded; the payment is a bank's action, and Vimbai never instructs one (ADR-005). The ledger entry that follows a decision is posted through `accounting.md`.

## Tests
The treasury analytics figures are asserted exactly in both the crate and the HTTP suite, including the 999-day runway floor and the thin-book case at 4000 per cent debt service. Cash optimisation, debt and foreign exchange are each unit-tested against a worked example, and the policy and risk paths are covered through their published prefixes.
