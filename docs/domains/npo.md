# Non-profit organisations

A Book that answers to donors and beneficiaries rather than to shareholders: restricted and unrestricted funds, grants, donations, and the statements a funder expects.

Crate: `rust/crates/domains/npo`

## What it owns

1. Donations, recorded with their amount in exact money, and refused when the amount is zero.
2. Grants: started active with nothing received, filtered by status, and read back by id. A grant that has been received against is a different report than one that has not, so the status is a fact rather than a label.
3. Net assets, and the statements of activities and of financial position, which are the two statements a funder reads first.
4. Donors, projects, programmes and their budgets, with the programme metrics that say what a programme actually did.
5. Internal controls and audit reports, and the social return on investment that turns programme outcomes into the figure a funder wants to see.
6. Volunteers, because a volunteer hour that is never recorded is a contribution the accounts cannot show.

One domain, one crate. `npo-scale-service` is the same arithmetic under a second prefix, not a second implementation.

## Services and addresses

| Service | Prefix |
| --- | --- |
| `npo-service` | `/npo` |
| `npo-scale-service` | `/npo-scale` |

## What it does not own

The surplus. A non-profit's "profit" is a change in net assets, and it comes out of the same ledger the for-profit side uses (`accounting.md`). Fund accounting records which fund a figure belongs to; the ledger records the figure.

## Tests

Fourteen tests over the published addresses: a donation recorded with its amount and a zero donation refused, donations listed under a count, a grant started active with nothing received, grants filtered by status, a grant read by id, and the statements answering from recorded figures.
