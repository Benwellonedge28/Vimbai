# Rust crate layout

> *Historical document.* This is snapshot of the crate layout. Current boundaries and the rules between them are in ADR-003 and, once written, dependency-rules.md.
>
> Kept for the record, not maintained. See `docs/ROADMAP.md` for what is being built now.

Three kinds of crate, and a service belongs in exactly one of them.

```text
rust/crates/common                 Money, shared primitives
rust/crates/kernel                 ledger core: posting integrity, hash chain
rust/crates/platform               identity, access control
rust/crates/calculations/<domain>  the maths, pure, no database
rust/crates/domains/<domain>       the services of that business domain
rust/crates/adapters               the record store and the Python bridge
rust/apps                          the API server and the local runtime
```

## One crate per business domain

A service used to be its own container carrying its own copy of the models, the database access and
the arithmetic. Porting them one at a time into a shared store would have repeated that in Rust:
fourteen modules side by side in the adapter, each reachable only through it. Instead every service
lives in the crate for its domain.

| crate | services |
| --- | --- |
| `domains/costing` | absorption-costing, basis-apportionment, cvp-analysis, job-costing, overhead-absorption-rate, process-costing, product-costing, selling-price-reduction |
| `domains/accounting` | cash-flow-statement, trading-account |
| `domains/treasury` | cash-optimization, treasury-analytics |
| `domains/budgeting` | appropriation-control |
| `domains/equity` | equity-changes |
| `domains/finance` | insurance-claims |

`tests/test_domain_crates.py` enforces it:

* the store holds no service module, and re-exports none;
* every service the catalogue calls Rust has a crate, and that crate's `lib.rs` names it;
* one crate per domain - no domain is split across two crates;
* every domain crate is `#![forbid(unsafe_code)]`.

Adding a service means adding a module to an existing domain crate, not creating a crate.

## The store is machinery

`crates/adapters/database` keeps the record shapes, the Book scoping, the aggregate helpers and the
Neo4j driver. It computes nothing. Every domain crate reads and writes through it, so the rules that
one Book can never write into another, and that every read is filtered by the same pair, are written
once.

Amounts arrive as a number or a decimal string and become a `Money`, which is exact at any
magnitude, through `vimbai_store::amounts` - the same reader for every service.

## Reading the port tracker

`rust/CALCULATOR-PORT.md` lists, under separate headings, the services served from Rust and the ones
still to do. Only the bullets under "Wired to the Rust library" count as ported. Both
`scripts/migration_manifest.py` and `scripts/migration_backlog.py` read it through
`migration_manifest.ported_names()`, and `tests/test_migration_backlog.py` asserts the reading in
both directions: a service served from Rust must be listed as wired, and one still to do must not be
counted.

