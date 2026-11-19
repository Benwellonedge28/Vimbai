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
| `invoice-customers-service` | `/customers` |
| `invoices-service` | `/invoices` |
| `invoicing-service` | `/invoices` |
| `purchases-ledger-control-service` | `/purchases-ledger-control` |
| `subscription-plans-service` | `/subscription-plans` |

## Discounts

A discount is a percentage or a sum of money, and it says who it is for. A rule is declared once, and it runs from the
day it starts to the day it stops: both days are required, because a discount that never ends takes money off invoices
nobody meant it to reach. A rule is
either compulsory - given on every invoice it fits without anybody asking - or offered, so the person raising an invoice
may take it or leave it. It fits every customer, or only the customers it names, and it can wait until an invoice reaches
a figure worth discounting.

Raising an invoice applies the compulsory rules that fit it, then any offered rule the caller named, in the order the
Book declared them: a percentage is taken off what is left at that point, a sum of money as it stands. The invoice keeps
one line per discount naming it and what it took off, so a customer can be told where their reduction came from, and the
figure that is left is what the customer owes and what tax follows. No rule may take the whole invoice: a discount
leaves something to pay. The rules are read at `/invoices/discounts`, and `/invoices/discounts/quote` answers what would
come off without raising anything.

## What it does not own

The posting. Raising an invoice produces a receivable; recording it in the ledger is `accounting.md`'s job, and the control account is derived from the entries rather than stored, so it cannot drift away from them. Nor does it take a payment - see `payments.md`.

## Tests

Contract tests cover an invoice raised against a customer and settled by a payment, and the settle path is exercised through the published address rather than the internal one. The purchases control reconciliation is tested against a subledger that deliberately disagrees, so the finding is proven to appear.
