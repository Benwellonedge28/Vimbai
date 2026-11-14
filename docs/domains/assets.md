# Assets

What a Book owns for longer than a year, and what it is worth now: depreciation, amortisation, revaluation, impairment, inventory value, and the registers that hold the answer between reports.

Crate: `rust/crates/domains/assets`

## What it owns

1. Fixed-asset registers and schedules: an asset's cost, its life, the method applied and the charge for the period, with the schedule kept so a later reader can see how the figure was reached rather than only what it is.
2. Depreciation and amortisation, including the disposal account that records an asset leaving with its profit or loss, and the arithmetic that decides it in exact money rather than by comparing floats.
3. Revaluation: the model, the reserve it feeds, and the transfer between them.
4. Inventory valuation, net realisable value, and goodwill impairment.
5. Intangible assets, lease termination, and asset disposal groups.
6. The two standards that are easy to get wrong and expensive to get wrong twice: investment property (IAS 40) under the fair value and cost models with a revaluation gain to other comprehensive income, and biological assets (IAS 41) with the three fair-value levels, bearer versus consumable classification, and the disclosure by hierarchy level.

Investment property and biological assets were calculators in the crate before anything answered the addresses their services published, so those addresses now serve the existing calculator rather than a second copy of the maths.

## Services and addresses

| Service | Prefix |
| --- | --- |
| `biological-assets-service` | `/biological-assets` |
| `disposal-account-service` | `/disposal-account` |
| `disposal-group-service` | `/disposal-group` |
| `fixed-assets-register-service` | `/fixed-assets-register` |
| `fixed-assets-schedule-service` | `/fixed-assets-schedule` |
| `intangible-assets-service` | `/intangible-assets` |
| `investment-property-service` | `/investment-property` |
| `lease-termination-service` | `/lease-termination` |
| `revaluation-model-service` | `/revaluation-model` |
| `revaluation-reserve-service` | `/revaluation-reserve` |

## What it does not own

The posting of a disposal or a revaluation to the ledger. This crate computes the charge and the gain; `rust/crates/kernel/ledger-services` records them, because a valuation that writes its own journal is a valuation nobody can reconcile.

## Tests

Eleven HTTP tests walk the published addresses: three fair-value levels, bearer versus consumable classification, disclosure, a full herd measurement, the fair-value model with a 40000 revaluation gain to other comprehensive income, the cost model, a transfer at fair value with 140000 to other comprehensive income, disclosure by hierarchy level, and yields at 8 per cent gross, 5.6 per cent net and 600000 implied. The disposal suite proves the profit or loss is decided by exact comparison, and the arithmetic behind every figure is unit-tested in its module.
