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

- The store trait becomes the swap boundary in practice, not just in name. The trait was shrunk so it is semantic rather
  than `run(cypher)`; what is left of the raw door is described in the amendment of 2026-11-25 below. The order it was
  done in: the edge doors, then a Postgres implementation, then SQLite for the device.
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

## Amendment, 2026-11-25: two targets, one suite, many stores later

The decision above was ratified, and narrowed by the owner in three ways.

1. **Two targets, not four.** PostgreSQL for a deployment and SQLite for a device are the providers Vimbai offers and
   supports. Neo4j is frozen where the decision left it: not deleted, not extended, no new feature built on it, and it
   does not define the contract. It stays reachable through `vimbai-store-move`, which is how a Book written into an old
   Neo4j install is lifted out. Nothing is removed until the suite below passes without it.
2. **Many stores in one deployment is deferred, and the seam must stay ready for it.** Today one process opens one store
   and every Book lives in it. The shape that makes several possible later is a placement of Book to store resolved per
   request rather than at startup, which is only honest if a cross-store operation is refused in words instead of half
   done. No code may assume the store it holds is the only one a deployment will ever hold.
3. **Portability is proved by one suite, not claimed by one trait.** `rust/crates/adapters/database/tests/conformance/mod.rs`
   holds the rules every provider must keep, and `tests/every_store_keeps_the_same_rules.rs` runs them against the memory
   graph, the file store, SQLite, and Postgres wherever a server is available. It pins, in the store's own terms and
   naming no dialect: a record reads back as written; another Book sees nothing and cannot reach a record by id; another
   kind holds nothing; an update changes what it named and leaves the rest; a figure survives to the last digit; a listing
   holds every record of the Book once; a page from a place holds exactly the rows after it, in id order; a delete removes
   the record and a second delete is refused; an edge written is an edge reported, or the store refuses rather than
   answering nothing. A provider that fails any of these is not a provider Vimbai will run on.

Two rules follow from the amendment and bind future work.

- **A capability gap refuses; it does not degrade.** A store that cannot hold edges says so loudly, because a dump that
  quietly came out without relationships looks like a migration and is not one.
- **No general query or DDL translator, and no posting authority.** The portable surface is operations plus a declared
  capability, not SQL spanning vendors. `ledger-core` remains the only judge of a balanced entry; a storage layer that
  starts doing arithmetic is the failure this decision exists to prevent.

Still open, deliberately: read/write routing, replication and failover are delegated to the provider's own tooling rather
than built here, and managed-database schema handling (apply at deploy, or verify at boot and refuse when behind) is
recorded as intent rather than built.

### Amendment, 2026-11-25: the move is a verb, and a deployment may only verify its schema

Two gaps stood between "the store is swappable" and "a business can change database with ease".

**The move is a verb, not a script.** `vimbai-store-move` now says what it would carry before it carries it (`--plan`,
counts per kind and whether the destination is empty), refuses to pour a store into one that already holds records, because
restoring into an occupied store does not merge two books, it mixes them under ids that mean different things on each side
(`--into-occupied` is the only way past, and it has to be said), and writes a receipt (`--receipt`) naming both stores, what
arrived, what did not match and whether it was a rehearsal (`--rehearse`). A move that leaves no record of itself is a
claim, not evidence.

**A deployment may only verify its schema.** `VIMBAI_SCHEMA_MODE=apply|verify` decides whether this process may change a
store's shape. The default is apply, because a device and a self-hosted deployment should bring themselves up. `verify` is
for a managed database whose credentials cannot create tables: the store is expected to be migrated where the engine is
deployed, and a store found behind is refused with the version it is at and the one expected, naming the way out, rather
than half-migrated by an application that was never given the right. Both stores now decide through one function
(`migrations::decide`), so the device and the deployment cannot drift in what they do about a schema.

### Amendment, 2026-11-25: a store says what it can do

Portability without a capability vocabulary is a promise the stores cannot keep, because they are not equally capable
and the difference would surface as a Book that is quietly incomplete rather than as a refusal. A store now states three
things: its name as a refusal uses it, whether it can hold a relationship between records, and whether two writes can
stand or fall together. A store that says nothing is taken at the minimum, records only.

The first use is the move: a destination that cannot hold what the source carries is refused before anything is written,
naming what it lacks. That is the rule that detection advises and never decides, in the one place the engine already has
to make the choice.

### Amendment, 2026-11-25: Neo4j is read from, never written to

The engine still fell back to Neo4j whenever `VIMBAI_POSTGRES_URL` was unset, so a host that set neither variable landed
on the store this decision had already frozen, silently. That fallback is gone: a deployment is Postgres or it refuses to
start, naming the setting it wants. Neo4j stays reachable as a source for `vimbai-store-move`, which is what a business
still running one needs to get out, and is refused as a destination so a Book cannot be moved into a store the engine
will not open.

### Amendment, 2026-11-25: the raw door takes a statement only the store crate can mint

The door above was left open on the argument that it had one caller. It was closed instead, and closing it did not need
the Neo4j path to go away first.

`Graph::run` no longer accepts a string and a bag of values. It accepts a `Statement`, whose fields are private and whose
constructor is `pub(crate)`. A caller holding a store can create, read, update, list and delete records and cannot hand the
database a statement of its own: no handler, no service crate and no script can reach past the store's rules, because it
cannot make the thing the door asks for. The statements that do run are written in `query`, `read`, `write` and `dump`,
which is where the scoping rules already lived.

A store implemented outside the crate - the scripted fakes in the engine's tests, the move tool's drivers - can still read
what it is being asked, through `Statement::cypher` and `Statement::params`. Reading a statement is not writing one.

Two consequences worth stating plainly:

- The backends that refuse statements at all (SQLite, Postgres) still refuse them, and that refusal is now pinned inside
  the store crate, because a statement can only be minted there. Out in the engine's tests the same fact shows up as the
  door not being callable at all, which is the stronger statement.
- A content gate (`no_caller_can_hand_the_store_a_statement_of_its_own`) fails if any `fn run` in the workspace takes a
  bare Cypher string again, or if the token's fields become public. A type can be widened by one careless edit; the gate
  is what makes the widening a decision rather than an accident.
