# Banking

The Book's side of its bank: statements imported and matched, reconciliations completed, the bank relationship itself, and the feeds and sweeps that keep a cash figure current.

Crate: `rust/crates/domains/banking`

## What it owns

1. Bank feeds: a single transaction imported, a batch imported, an MT940 statement parsed into its credit and debit lines with their signs intact, the feed listed with its status, a feed linked, and a rule applied to a line. A duplicate import is refused rather than silently recorded twice - the comparison is on the transaction itself, not its arrival order.
2. Bank reconciliation, including the two steps a reconciliation is actually made of, so a partial reconciliation says which step it is at.
3. The bank relationship: mandates, signatories, limits and the changes to them.
4. Banking integration: per-connection transaction feeds and per-connection sweeps, so one connection's import cannot sweep another's.
5. Currency: exchange rates, the conversions they imply, and the currency catalogue a multi-currency Book works in.

## Services and addresses

| Service | Prefix |
| --- | --- |
| `bank-feed-service` | `/bank-feed` |
| `bank-reconciliation-service` | `/bank-reconciliation` |
| `bank-relationship-service` | `/bank-relationship` |
| `banking-integration-service` | `/banking-integration` |
| `currency-service` | `/currency` |

## What it does not own

Moving money. Vimbai records what a bank did and what a Book decided; it does not instruct a payment. See `payments.md` for the same rule stated about tills. Nor does it own the ledger posting of a reconciled item: the reconciliation produces the figure, and the accounting crate posts it.

## Tests

Five HTTP tests cover the feed: an exact amount kept with a duplicate refused, a batch reporting both lines, an MT940 statement parsed into a credit and a debit read back with signs, a fuel rule meeting the line it names, and the feed listing with status. The per-connection feed and sweep are covered by the address suite that closed the last method gaps, and currency conversion is unit-tested against exact amounts.
