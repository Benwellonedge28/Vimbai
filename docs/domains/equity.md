# Equity and payroll

What the owners hold, how it changed, and what the people who work for the Book are owed: share capital in every form, reserves, dividends, the equity statement, and the payroll that pays for it.

Crate: `rust/crates/domains/equity`, `rust/crates/domains/payroll`

## What it owns

1. Share capital by class: ordinary and preference shares, issued and authorised capital, share premium, bonus shares, rights issues (accepted and renounced), redemptions and the capital redemption reserve.
2. Reserves: the general reserve, retained profits, and the appropriations that move profit into them.
3. Distributions: ordinary and interim dividends, the declaration and the payment, with pay answering both the published POST and the PUT an older client sends.
4. Capital reconstruction and debentures.
5. The statement of changes in equity, which is where all of the above adds up.
6. Payroll: the pay run, its deductions, and the benefits administered alongside it.

The earnings figures are exact: diluted shares and diluted earnings per share are computed in `Money` and the parity test asserts the exact figures deliberately instead of mirroring a float drift.

## Services and addresses

| Service | Prefix |
| --- | --- |
| `authorized-share-capital-service` | `/authorized-share-capital` |
| `benefits-admin-service` | `/benefits-admin` |
| `bonus-shares-service` | `/bonus-shares` |
| `capital-reconstruction-service` | `/capital-reconstruction` |
| `capital-redemption-reserve-service` | `/capital-redemption-reserve` |
| `debentures-service` | `/debentures` |
| `equity-changes-service` | `/equity-changes` |
| `general-reserve-service` | `/general-reserve` |
| `initial-investment-service` | `/initial-investment` |
| `issued-share-capital-service` | `/issued-share-capital` |
| `ordinary-shares-service` | `/ordinary-shares` |
| `preference-shares-service` | `/preference-shares` |
| `retained-profits-service` | `/retained-profits` |
| `right-issues-service` | `/right-issues` |
| `share-premium-service` | `/share-premium` |
| `share-redemption-service` | `/share-redemption` |

## What it does not own

The bank transfer that pays a dividend. Vimbai records the declaration and the liability; moving the money is not its business. Nor does it decide the tax on a distribution, which is `taxation.md`.

## Tests
The equity arithmetic is unit-tested per instrument, including the dividend pay path in both methods and the preference-class create that publishes an address nothing answered until the address gate found it. The changes-in-equity statement is checked against its inputs, and the payroll run against its deductions.
