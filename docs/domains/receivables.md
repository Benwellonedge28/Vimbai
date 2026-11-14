# Receivables

What customers owe, what of it will not be collected, and the controls that prove the debtors ledger agrees with the ledger it sits beside.

Crate: `rust/crates/domains/receivables`

## What it owns

1. The provision for doubtful debts, recalculated from the debtor register rather than recorded as a figure somebody typed. That distinction is the point: a provision that is not derived from the register drifts from the debts it exists to cover.
2. The aged provision, which asks how old a debt is and provides accordingly, and which answers on its own address rather than borrowing the other provision's handler.
3. Bad-debt recovery: a debt written off and later recovered, with the recovery recorded against the debt rather than as a fresh sale.
4. The sales ledger control, which reconciles the debtors subledger against the sales control account.

## Services and addresses

| Service | Prefix |
| --- | --- |
| `bad-debts-recovery-service` | `/bad-debts-recovery` |
| `provision-bad-debts-service` | `/provision-bad-debts` |
| `provision-doubtful-debts-service` | `/provision-doubtful-debts` |
| `sales-ledger-control-service` | `/sales-ledger-control` |

## What it does not own

The invoice that creates a receivable - `invoicing.md` - or the posting that records the provision. The provision is computed here and journaled through the accounting crate, so the arithmetic and the entry stay separate.

## Tests

The provision suite proves the figure is recalculated from the register rather than recorded from the request body, and that two doubtful-debt addresses answer from their own handlers rather than each other's - a silent wrong answer that the address gate caught. The control reconciliation is tested against a subledger that disagrees.
