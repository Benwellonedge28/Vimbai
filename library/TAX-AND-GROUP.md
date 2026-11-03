# tax_reporting and group_accounting

Two crates, safe Rust, stateless: no database, no globals, no clock. Dates the
responses need arrive from the caller.

Parity figures were captured by running each Python service on canonical inputs
and are asserted in the crate tests. Where a service carried a defect, the test
names it and the code preserves it, because filed figures must not move under a
port.

## vimbai_tax_reporting (7 services)

| module | service | notes |
| --- | --- | --- |
| `credit` | r-and-d-tax | tie-break recommends alternative simplified |
| `group` | group-tax | savings formula nets off the unpaid standalone shortfall |
| `vat` | vat-reporting | a line is taxed at its stated rate whatever its type |
| `provision` | tax-provision | deferred expense is dtl - dta, so a net DTA is negative |
| `risk` | tax-risk | banded levels, five standing recommendations |
| `grants` | government-grants | income grants still disclose "over N years" |
| `xbrl` | xbrl-reporting | five required concepts or the filing is invalid |

## vimbai_group_accounting (6 services)

| module | service | notes |
| --- | --- | --- |
| `partnership` | partnership-accounting | residual loss floored at zero; zero shares split equally |
| `buyout` | partnership-sale | `difference` and `gain_or_loss` are mirrors |
| `combination` | business-combination | goodwill reported even when not recognised |
| `impairment` | goodwill | goodwill absorbs the loss before other assets |
| `pension` | pension-accounting | zero actual return falls back to the expected return |
| `consolidation` | consolidation | see below |

### consolidation is a deliberate departure

The Python service had no input for the parent company. It hardcoded revenue
10,000,000, profit 1,500,000, assets 50,000,000 and liabilities 30,000,000,
returned a fabricated cash flow statement (2,000,000 / -800,000 / -500,000, net
710,000), charged a flat 50,000 of goodwill amortisation, split assets 60/40 and
liabilities 40/60, multiplied NCI by 1.2 "at fair value", and defaulted missing
subsidiary figures to 2,000,000 / 300,000 / 5,000,000 / 2,500,000. None of that
can sit in a ledger, so the parent is now an input and the invented lines are
gone. The real maths is unchanged.

Two things to decide, both flagged rather than fixed:

1. Subsidiary revenue and profit are included in proportion to ownership, which
   is proportionate consolidation, yet NCI is also deducted. The two do not
   belong together.
2. NCI's profit share is taken of the already-ownership-scaled profit, so a 60%
   subsidiary charges the minority 40% of 60% instead of 40% of 100%. Group
   profit attributable to the parent is overstated. Equity does not have the
   fault. Fixing either changes every consolidated figure ever filed.
