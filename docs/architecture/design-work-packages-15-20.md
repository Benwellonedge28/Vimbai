# Vimbai design work packages 15–20

Source review [2df3a76](https://github.com/Benwellonedge28/Vimbai/commit/2df3a7685b1109756fad0cca4d7b648b25790bdb). Continues [packages 01–08](design-reading-order.md) and [packages 09–14](design-continuation-order.md). Read each design’s prerequisites before scheduling it.

| Order | Design | What it enables |
| --- | --- | --- |
| 15 | [Business document evidence](../domains/business-document-evidence.md) | Verified attachments for Book recovery and store moves |
| 16 | [Book sync at scale](book-sync-at-scale.md) | Bounded catch-up and durable replay receipts |
| 17 | [Foreign-currency settlement](../domains/foreign-currency-settlement.md) | Repeatable carrying amounts, allocations and FX evidence |
| 18 | [Cross-Book consolidation](../domains/cross-book-consolidation.md) | An approved group statement from authorised source Books |
| 19 | [Verified store cutover](verified-store-cutover.md) | A consistent migration with a completeness and rollback receipt |
| 20 | [Hosted and self-hosted release](hosted-and-self-hosted-release.md) | An installable, recoverable web and backend deployment |

Packages 15–17 can be reviewed independently once their earlier prerequisites are agreed. Package 18 consumes 17. Package 19 consumes the evidence and cursor contracts in 15 and 16. Package 20 consumes 19 for live upgrades and recovery. These proposals specify implementation and tests for these six packages. They do not record completed implementation.
