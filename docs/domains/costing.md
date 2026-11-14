# Costing

What a unit of output costs, and the arithmetic that decides it: absorption, job, process and product costing, overhead apportionment, break-even, and the two decisions that follow from a cost - whether to keep making something, and what to charge for it.

Crate: `rust/crates/domains/costing`

## What it owns

1. Prime cost, production cost and the cost per unit, from one exact-money implementation. The seam fixture records it: prime cost 2300, production cost 3000, unit cost 30.
2. Overhead absorption and the absorption rate: 84000 of overhead over 28000 hours absorbs exactly 84000, and short hours leave -12000 under-absorbed rather than a rounding artefact.
3. Basis apportionment: rent split by floor area, 480 and 720 of 1200, with the parts summing to the whole; six other bases reading each centre's own figure; a standing rule kept and listed.
4. Job costing, process costing and product costing, and the absorption-costing statement and selling-price reduction that read them.
5. CVP analysis: fixed cost, contribution per unit, the break-even point and the margin of safety.
6. The continue-or-shutdown decision, answered as CONTINUE, SHUTDOWN or INDIFFERENT with exact money, never as a float comparison that would call a tie a shutdown.

Absorption costing is also the seam oracle: the figures the retired Python service answered are pinned in the seam tests, so the Rust implementation is checked against a recorded answer rather than against itself.

## Services and addresses

| Service | Prefix |
| --- | --- |
| `absorption-costing-service` | `/absorption-costing` |
| `absorption-costing-statement-service` | `/absorption-costing-statement` |
| `basis-apportionment-service` | `/basis-apportionment` |
| `cvp-analysis-service` | `/cvp-analysis` |
| `job-costing-service` | `/job-costing` |
| `overhead-absorption-rate-service` | `/overhead-absorption-rate` |
| `process-costing-service` | `/process-costing` |
| `product-costing-service` | `/product-costing` |
| `selling-price-reduction-service` | `/selling-price-reduction` |

## What it does not own

The decision to act on a cost. This crate says a product costs 30 and sells at 300 with a 20 per cent markup; whether to make it is a person's decision recorded elsewhere. It also does not own the overhead account in the ledger, only the absorbed figure.

## Tests

Three absorption tests pin the figures above end to end over HTTP; eight more walk the apportionment and absorption-rate addresses, base by base; seven cover the shutdown decision including a tie and the quick analysis that stores nothing; and the CVP, job, process and product calculators each carry their own unit tests. The duplicate gate caught a markup bug multiplying by twenty instead of a fifth of cost before it landed.
