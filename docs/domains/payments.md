# Payments

This page exists to say what is *not* here, because the answer is easy to get wrong from the outside.

Crate: `rust/crates/domains/payments`

## What it owns

The point-of-sale till. A terminal is registered, a taking is recorded against it, the queue of takings is emptied in one batch with a repeat refused rather than counted twice, and a taking is marked synced once it has left the till. That is the whole of it: Vimbai records a sale that has already happened at the counter.

## Services and addresses

| Service | Prefix |
| --- | --- |
| `pos-integration-service` | `/pos-integration` |

## What it does not own

Moving money, and Vimbai Pay. ADR-005 records the decision: Vimbai Pay is not built and is not on the roadmap. There is no wallet, no balance held on a customer's behalf, no card acquirer integration, no settlement and no payout anywhere in this repository. Vimbai records that money moved; it does not move it, and a service that appears to is either recording a decision or is a defect.

The rule is not a limitation waiting to be lifted. It is why a payment integration cannot be half-built here without contradicting the ledger's own authority over what a balance means.

## Tests

Three tests cover the till: a terminal registered, a taking recorded, the queue emptied in one batch with a repeat refused, and a taking marked synced. The duplicate-address gate proves no second implementation answers the same address.
