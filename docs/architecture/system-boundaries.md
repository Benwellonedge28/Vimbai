# System boundaries

What each part of Vimbai owns, and where one part stops and the next begins. One page, because a boundary nobody can
hold in their head is a boundary that moves.

## The four movements of a request

```
Flutter  ->  Go gateway  ->  api-server (Rust)  ->  domains  ->  ledger-core  ->  store
```

Each arrow is a boundary with a rule attached, and each rule is enforced somewhere in `rust/xtask` rather than agreed in
a conversation.

## The client

Flutter in `apps/flutter`, for mobile, desktop and web. It owns presentation, local caching and the offline queue. It
does not compute a financial figure that the engine also computes: if a screen needs a total, it asks for it, because two
implementations of the same number is how the Python estate lost its arithmetic.

## The gateway

`gateway/go`. Routing, authentication, authorisation, rate limiting, resilience, observability. It is the only component
a client addresses, and the only place a token is read.

It carries *no* accounting arithmetic, no ledger posting and no settlement logic (ADR-001). When a defect looks like it
belongs there, it belongs one layer in. Its route table, `gateway/go/config/services.json`, is the published address
inventory; `catalog/services/catalog.yaml` is generated from it and pinned by a test.

## The API server

`rust/apps/api-server`. It owns the HTTP surface: request shapes, status codes, the error envelope (ADR-004), the Book
scope headers, and the decision of which domain answers a call. It parses, it refuses, it delegates. Business rules stay
behind it, which is why the same functions are callable from tests without HTTP.

## The domains

`rust/crates/domains/*`, one crate per business domain (ADR-003). A domain owns the rules of its domain: what a valid
invoice is, when a period may close, which correction is allowed. It orchestrates the kernel and the calculators, and it
is the only layer allowed to decide that something should be posted.

A domain does not import another domain (rule R5). When two domains must work together, the composition happens a layer
up, in `crates/application` or in the API server, so the coupling is visible in one place instead of woven through both.

## The ledger kernel

`rust/crates/kernel/ledger-core` is the sole authority for posting integrity: the entry balances, the entry is
immutable, a reversal is an exact mirror, and the hash chain extends over every posted entry (ADR-002). It depends on
`common` and nothing else, so it cannot borrow a rule from anywhere.

`kernel/ledger-services` is its reading half: control-account reconciliation, integrity reporting and the financial state
machine. It reads through the store, which is the one tolerated dependency in the workspace and is recorded with its
reason in `dependency-rules.md`.

## The calculators

`rust/crates/calculations/*`, sixteen crates of arithmetic with no state: given amounts, return amounts. They may not
reach a domain, a platform crate, an adapter or an app (rule R3). This is the layer that exists because duplication of
arithmetic was the original sin of the estate being replaced.

## Platform and adapters

`rust/crates/platform/*` provides what every domain needs and none of them should own: identity, access control, events,
documents, notifications, integrations, offline sync, cache, alerts, webhooks, admin.

`rust/crates/adapters/*` is the edge. `database` is the property store: records, Book scoping, amounts, the driver, and
nothing that computes a business figure. `wire` is the JSON seam that lets a calculator answer over HTTP.

## Tooling

`rust/xtask`. Gates and generators that read the tree: the catalogue, the address audit, the reachability report, the
dependency rules, and the repository rules. It depends on no workspace crate (rule R8), so it can fail the workspace
without being part of it.

## Where a new piece of work goes

1. Arithmetic with no state: `calculations/<name>`.
2. A rule about the business: the existing domain crate for that part of the business, or a new one under `domains/`.
3. Something every domain needs and none should own: `platform/`.
4. A way to reach storage or the network: `adapters/`.
5. A use case that composes several domains: `crates/application`, or the API server if it is one endpoint.
6. A program: `apps/`.

Nothing in that list is a service. There is one engine, one gateway and one app (ADR-003), and a new directory is a
decision to be argued rather than a default to be reached for.
