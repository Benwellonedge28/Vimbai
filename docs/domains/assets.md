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

### Three ways of writing an asset off

An asset is written off by one of three methods, and the choice is the asset's own, recorded on its register row: a business can write a building down the same every month and a van by a share of what is left, in one register.

1. `straight_line` charges `(cost - salvage) / (life * 12)` a month, whatever the calendar says.
2. `reducing_balance` charges a share of what is left each year, twelve times over, so the charges are bigger now and smaller later. The share is the asset's stated `depreciation_rate` when it has one, and otherwise the life's own (`100 / life` years). The shipped 20% a year was only right for a five-year asset: it wrote a ten-year one off in five.
3. `time_based` charges the annual figure for the days of the period the asset was actually held, counting 365 days to a year. A van bought on the 20th is charged for the part of that month it was in the yard, and its last period is a part-period too.

No method takes an asset below what it is expected to be worth at the end: the last charge is exactly what is left, so accumulated depreciation and net book value on the register always add back to the cost. A method the register does not know is refused in words naming the three, rather than charging nothing: a register that quietly takes nothing off an asset reads exactly like one where the asset never wears out.

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

Six more walk the register's three methods over a real Book: a time-based van charged only for the days it was held, a straight-line one charged a whole month, a reducing-balance one charged its stated rate, all three sitting side by side in one register, an unknown method refused in words, and a month before a time-based asset arrived refused rather than charged nothing.

Eleven HTTP tests walk the published addresses: three fair-value levels, bearer versus consumable classification, disclosure, a full herd measurement, the fair-value model with a 40000 revaluation gain to other comprehensive income, the cost model, a transfer at fair value with 140000 to other comprehensive income, disclosure by hierarchy level, and yields at 8 per cent gross, 5.6 per cent net and 600000 implied. The disposal suite proves the profit or loss is decided by exact comparison, and the arithmetic behind every figure is unit-tested in its module.
