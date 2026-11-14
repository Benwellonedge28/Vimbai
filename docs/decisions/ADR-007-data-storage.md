# ADR-007: One system of record, and a way off it

*Status:* accepted, 2026-11-14. Closes the phase P8 was blocked on.

## Context

The store sits behind one trait (`vimbai_store::Graph`) and three implementations exist: Neo4j for a deployment, a file for
one machine, and an in-memory one for tests. The corpus answers from Neo4j today and is green. The question was never
"does it work" but "is this the right database for a ledger", and the honest answer was: it fits Vimbai's *shape* and not
Vimbai's *integrity requirements*.

What the platform actually stores is shallow: `HAS_LINE`, `DEBITED`, `CREDITED`, `OWNS_X`, always one hop, always
book-scoped. There is no variable-depth traversal, no path ranking, no pattern matching. Neo4j is a graph database being
used as a record store with very short edges.

What it does not give us, and a ledger needs:

1. **No decimal type.** Amounts are text and every sum happens in Rust. That works - `Money` exists for it - but the
   database can never aggregate a Book, so reconciliation always means loading rows into the engine.
2. **No row lock for `book_seq`.** Concurrency is carried by our claim and idempotency layers instead of by the database.
3. **The free edition stops where availability starts.** Clustering, hot backup and fine-grained security are Enterprise
   only, so the free version is fine on one machine and becomes a licence at the first high-availability requirement.

## Decision

1. **PostgreSQL is the system of record for a deployment.** Exact `numeric` for money, real transactions spanning an entry,
   its lines and its edges, row locks per Book, and idempotency as a unique index rather than a table we maintain. Free
   without a feature ceiling, including replication and point-in-time recovery.
2. **SQLite is the system of record on a device.** One file, no server, exact decimals carried as text and summed by
   `Money`, which is already the rule everywhere.
3. **Neo4j is not deleted and not extended.** It remains a working backend behind the trait, useful as a read model if
   traversal ever grows past one hop, and useful as evidence that the trait is real. No new feature is built on it.
4. **The move is a tool, not a rewrite, and it exists before Postgres does.** `vimbai-store-move` takes every record and
   every edge out of one store and writes them into another, compares the arrival against the source, and refuses rather
   than reporting success it cannot prove. Edges travel as data: a restore that re-derived relationships from fields would
   rebuild yesterday's data with today's rules.
5. **Aggregation stays in Rust on every backend.** Postgres could total a Book in `numeric`; the other two cannot. One
   summing rule (`Money::sum`) used everywhere is worth more than a faster sum on one backend that makes the profiles
   disagree.

## Consequences

- The store trait becomes the swap boundary in practice, not just in name. Work still to do, in order: shrink the trait so
  it is semantic rather than `run(cypher)` (the two edge doors added with the move are the first of those), then a Postgres
  implementation, then SQLite for the device.
- A Postgres adapter cannot be written honestly without a server to test against. It was built only once a Postgres was
  available, with the round-trip gate pointed at it: eleven tests against Postgres 15, including a Book carried from
  SQLite into Postgres and compared on arrival. Two defects only a real server could show were fixed on the way, and both
  are recorded here because they are properties of this decision rather than of one file:
  - *The client must not be driven from inside another runtime.* The synchronous Postgres client runs its own runtime, so
    a query issued on the engine's async thread panicked with "cannot start a runtime from within a runtime". Every
    operation now runs on a thread of its own, which also means a slow query cannot stall other requests. Any future
    backend with a synchronous client has the same constraint.
  - *A `jsonb` column takes a value, not its text.* Postgres infers the parameter type from the target column and refuses
    to convert a string into `jsonb`; the record travels as a value. The same applies to anyone adding a typed column.
- Preflight starts a throwaway Postgres when the server binaries are installed, so these tests run rather than skipping. A
  test that skips is a test nobody runs.
- The data is movable today, which is the property that made deferring safe: nothing about this decision is expensive to
  reverse.
- One thing this ADR does *not* decide: ownership of the 180 prefix groups with no catalogue entry. That is a documentation
  gap, not a storage one.
