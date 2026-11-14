# Taxation

The tax the Book owes, the planning that decides what it will owe, and the compliance that proves both were handled - kept on its own prefix because a tax word shared with another domain is not a shared address.

Crate: `rust/crates/domains/taxation`

## What it owns

1. Tax accounting: the tax charge for a period, the deferred position, and the entries that carry them.
2. Tax planning: the arrangements considered and the saving each implies, recorded as a plan rather than applied silently.
3. Tax compliance: the obligations, the filings and their status, answering on its own prefix.

Tax arithmetic is exact money. A rate applied to a float and rounded at the end is a tax figure that will eventually disagree with the ledger by a cent, and a cent in a tax return is a conversation with a revenue authority.

## Services and addresses

| Service | Prefix |
| --- | --- |
| `tax-accounting-service` | `/tax-accounting` |
| `tax-compliance-service` | `/tax-compliance` |
| `tax-planning-service` | `/tax-planning` |

## What it does not own

The tax law itself. No rate table in this repository claims to be current: a rate is an input a Book supplies, and the crate applies it. Nor does it own the distribution decision - `equity.md` handles dividends and this crate handles their tax treatment.

## Tests

Each calculator is unit-tested against a rate supplied by the test, so a change in a real rate cannot silently move a test's expectation. The compliance path is covered through its published prefix, which is what keeps the tax words from colliding with another domain's addresses.
