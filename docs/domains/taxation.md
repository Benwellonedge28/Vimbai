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

## How a document carries its tax

A document amount is the gross figure: what the customer pays, or what a bill comes to. Tax is taken out of it, never
added on top, so a 100,000.00 sale at 15.5% is 86,580.09 of income and 13,419.91 of tax that add back to exactly
100,000.00.

A discount comes off that gross figure before any tax is worked out. A customer given 5,000.00 off a 100,000.00 sale
paid tax on 95,000.00, not on the 5,000.00 they were never charged: `compute_tax` deducts the discount and then extracts
each tax from what is left, and `create_sale` has always posted the receivable net of the discount and extracted the tax
from the same figure, so the reading the phone previews and the entry the ledger receives cannot disagree. A discount
that swallows the whole document is refused, because a document with nothing left has nothing to carry tax.

## What it does not own

The tax law itself. No rate table in this repository claims to be current: a rate is an input a Book supplies, and the crate applies it. Nor does it own the distribution decision - `equity.md` handles dividends and this crate handles their tax treatment.

## Tests

Each calculator is unit-tested against a rate supplied by the test, so a change in a real rate cannot silently move a test's expectation. The compliance path is covered through its published prefix, which is what keeps the tax words from colliding with another domain's addresses.
