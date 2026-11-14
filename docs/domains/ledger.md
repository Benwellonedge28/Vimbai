# The ledger kernel and the calculation surface

The one authority for whether a journal entry may exist, and the addresses that let a client ask for a reckoning without knowing which crate holds the arithmetic.

Crates: `rust/crates/application`, `rust/crates/kernel/ledger-services`

## What it owns

`ledger-core` decides four things and nothing else: that an entry balances, that a posted entry cannot be edited or deleted, that a reversal is the exact mirror of what it reverses, and that the hash chain over posted entries is intact. It is deliberately small, because a rule that lives in two places is a rule that disagrees with itself.

The precision door matters more than it sounds. `validate_money_lines` compares exact `Money`; `validate_lines` is the float door and rounds to ten decimal places so that 0.1 + 0.2 reads as 0.3. The old 1e-9 tolerance is gone: a one-unit difference in the thirtieth decimal place is refused, where a float would have posted it.

The calculation surface is the other half of this page. `calculation-api` answers the arithmetic questions any client asks - net present value, internal rate of return, payback period, depreciation, a loan amortisation schedule - by routing to the single implementation in the crate that owns it, and `realtime-calculation-engine` is that same surface under its published prefix. Neither keeps a copy of a calculator: `/npv` is `finance::discounting::net_present_value`, and a second copy would be a second answer waiting to disagree.

## Services and addresses

| Service | Prefix |
| --- | --- |
| `calculation-api` | `/calcs` |
| `control-account-reconciliation-service` | `/control-account-reconciliation` |
| `double-entry-principles-service` | `/double-entry-principles` |
| `financial-integrity-service` | `/financial-integrity` |
| `financial-state-machine-service` | `/financial-state-machine` |
| `realtime-calculation-engine` | `/realtime-calculation` |

## What it does not own

Any business rule about what an entry means. The kernel does not know what a sale is, and a domain that asks it to is asking the wrong crate. No calculation lives here either: this page owns the door, and the calculations crates and the domain crates own the arithmetic behind it.

## Tests

The financial-integrity suite at `rust/tests/financial-integrity` sits outside the kernel on purpose, so a failing rule is not checked in the same file that implements it: balance, immutability, reversal-nets-zero, hash chain, and property tests over a seeded generator. That suite is the reason this authority can be trusted, and `docs/compliance/financial-integrity.md` names the test behind each claim.
