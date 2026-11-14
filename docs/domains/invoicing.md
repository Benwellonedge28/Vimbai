# Invoicing and payables

What the Book bills, what it owes, and the ledger controls that prove each side agrees with the ledger it sits beside.

Crates: `rust/crates/domains/invoicing`, `rust/crates/domains/payables`

## What it owns

1. Invoices raised against a customer, settled by a payment, and the state between the two. An invoice is addressed by its number, not by an internal id - a trap worth stating, because the parameter looks like an id and is not one.
2. Subscription plans: what a customer is on, and what that entitles them to charge next.
3. The purchases ledger control, which proves the payables subledger agrees with the purchases control account in the ledger.
4. Expense tracking, which answers on its own prefix because the flat summary address belongs to a different service and only one service may hold it.

## Services and addresses

| Service | Prefix |
| --- | --- |
| `expense-tracking-service` | `/expense-tracking` |
| `invoicing-service` | `/invoicing` |
| `purchases-ledger-control-service` | `/purchases-ledger-control` |
| `subscription-plans-service` | `/subscription-plans` |

## What it does not own

The posting. Raising an invoice produces a receivable; recording it in the ledger is `accounting.md`'s job, and the control account is derived from the entries rather than stored, so it cannot drift away from them. Nor does it take a payment - see `payments.md`.

## Tests

Contract tests cover an invoice raised against a customer and settled by a payment, and the settle path is exercised through the published address rather than the internal one. The purchases control reconciliation is tested against a subledger that deliberately disagrees, so the finding is proven to appear.
