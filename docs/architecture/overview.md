---
title: Architecture overview
summary: The platform as built, read from the finished tree rather than from the proposal it came from.
---

# Architecture overview

This is what the repository is now. It is written from the code, not from the design document, and where the two differ
the difference is stated.

## One engine, one gateway, one app

The platform answers 138 routed services. Every one of them is served by one Rust engine
(`rust/apps/api-server`): the arithmetic, the records and the HTTP surface all live there. A service is a set of
addresses the engine answers, not a process.

Three things face the engine, and each has one job:

| Component | Crate | What it does | What it must not do |
|---|---|---|---|
| Engine | `rust/apps/api-server` | answers every published address | hold a payment provider |
| Gateway | `gateway/go` | routing, authn/authz, rate limiting, resilience, proxying, observability | contain accounting or ledger logic |
| Sync server | `rust/apps/sync-server` | the Book surface for devices, 404 to everything else | post a journal entry |
| Worker | `rust/apps/worker` | runs what is due, exactly once | hold a schedule in memory |
| Local runtime | `rust/apps/local-runtime` | the same engine on one machine, offline | need a server it was not given |

The app is one Flutter codebase at `apps/flutter`, organised by feature. The React front end is gone as a UI path.

## The workspace, by family

```
rust/crates/
  kernel/        ledger-core (the only posting authority), ledger-services, identity, tenancy
  money/         common: Money, exact decimals, the shared vocabulary every crate speaks
  domains/       one crate per business domain: accounting, treasury, costing, equity, tax, npo, ...
  calculations/  the arithmetic the domains call, with no records and no HTTP
  platform/      tenancy, documents, automation, gateway adapters
  adapters/      database, wire: how records are stored and how JSON crosses the edge
rust/apps/       api-server, worker, sync-server, local-runtime
rust/tests/      financial-integrity, scale: what must hold, proven from outside
rust/xtask       the gates, as a library, because a gate reused is a gate that cannot disagree with itself
```

Two rules hold the shape. A dependency may only point along the families above (`cargo run -p xtask -- boundaries`
prints the table and fails on a violation). And no crate may contain `unsafe` code: every library crate carries
`#![forbid(unsafe_code)]`, checked over the tree rather than trusted per file.

## Money

Every amount is `vimbai_common::Money`: a `BigDecimal` newtype with unbounded digits and magnitude, serialised as a JSON
string so a value never passes through a float on the way to a client. Sums, allocations and comparisons are exact; there
is one summing rule (`Money::sum`) and one allocation rule (`Money::allocate`), because two would eventually disagree.

The float door still exists at the edge - a JSON body may carry a number - and it is deliberately conservative: it rounds
to the decimals the number was written with, and the financial-integrity suite proves that nothing the float door accepts
is refused by the exact door.

What that buys, and what the scale suite proves: forty-digit amounts survive storage and read back unchanged, twenty
thousand thirty-digit addends total the same in any order, an allocation of a thirty-digit amount across a thousand parts
adds back to the whole to the last minor unit, and sixteen - or a hundred - writers on one Book lose nothing and keep
their large amounts intact.

## Tenancy

An organisation is a platform record. A Book belongs to exactly one organisation, and a member of that organisation
reaches its Books without a membership row of their own. One function decides it (`tenancy::access::allows`), and the
engine applies it in its own gate rather than trusting the gateway. A stranger asking for a Book gets a 403, not an empty
Book: "no records" and "not yours" are different answers and the client needs to tell them apart.

## Records

The store is a graph, reached through one trait (`vimbai_store::Graph`). A record carries `user_id` and `book_id` stamped
from the caller's scope, never from the payload: a caller that thinks it is writing somewhere else hears about it.
Journal lines are records of their own, joined by `HAS_LINE` and linked to their account by `DEBITED` or `CREDITED`, so a
posting is replayable from the store.

Shared ledger entries are immutable. A correction is a reversing entry, and the kernel refuses the rest.

## Where it differs from the design document

1. **No `crates/money`, `identifiers`, `errors` or `contracts`.** `common` owns all four. Splitting them adds three
   dependency edges and no separation (ADR-008).
2. **No `cost_accounting` rename.** The crate directories keep underscores; the hyphenated names in the proposal are a
   directory preference, not a technical requirement (ADR-008).
3. **Vimbai Pay does not exist.** The platform records money; it does not move it (ADR-005).
4. **The developer platform is deferred, not cancelled.** Its trigger is the first real request for API credentials
   (ADR-006).
5. **180 prefix groups answer with no catalogue owner.** Retired services' addresses still answer so that no client
   broke when the service went; they are documented path by path in `contracts/openapi/v1/domains/unattributed.yaml`
   (826 addresses) and attaching them to owners is an open item, not a claim. The gateway routes 137 prefixes while the
   engine answers addresses under 290, which is the same fact counted the other way.
