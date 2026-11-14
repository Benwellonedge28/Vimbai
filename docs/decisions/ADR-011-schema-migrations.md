# ADR-011: A schema change is a numbered step, recorded, in both stores

**Status:** accepted, 2026-11-14
**Amends:** ADR-007 (one system of record, SQLite on a device)
**Depends on:** nothing

## The problem

The device store had migrations and the deployment store did not. SQLite carried a numbered list of steps with a `down`
for each, a `schema_version` row, a refusal for a store written by a newer build, and a test that applied, undid and
re-applied the schema over records that were already there. Postgres ran one block of `CREATE TABLE IF NOT EXISTS` on
every open and kept no version at all.

That is the difference between a store that can change and one that can only be recreated. On a deployment holding a
real ledger, "run the create statements again" does nothing once the tables exist, so the next schema change would have
been discovered as a missing column at the first write rather than as a migration, and two deployments of the same
version of the platform could sit on two different shapes with nothing to compare.

The two backends also each decided what to do with a version they did not recognise. Two answers to that question is how
one of them ends up writing to a schema it does not understand.

## The decision

1. **The schema is a list of numbered steps.** Each step has a name that carries its own number (`0002_edges`), the SQL
   that applies it, and the SQL that undoes it. Steps are appended; a step that has shipped is never edited, because an
   edited step is a different step on a store that already ran it.
2. **The version is recorded in the store**, in a `schema_version` row, written in the same transaction as the step. A
   step that fails half way therefore leaves the previous version, not a half-built schema, and the two can never
   disagree.
3. **One function decides what to do**, `migrations::plan`, shared by both backends: run the missing steps, run nothing,
   or refuse. It is a pure function over `(the version found, the steps this build has)` and is tested on its own.
4. **A store written by a newer build is refused, not downgraded.** Running the steps this build knows against a schema
   that already has steps it cannot see would record a lower version than the schema actually is, and the next step
   would be written against a shape it never expected. The refusal says to upgrade the engine rather than to open it with
   an older one.
5. **Both stores carry the same steps in the same order.** The SQL is each backend's own business - Postgres says
   `jsonb`, SQLite says `TEXT` - but the steps are the platform's, so a change made in one backend and not in the other
   is a defect. A test compares the step names in order and fails if they diverge; it needs no server.
6. **Every step can be undone.** A step whose `down` does not work is a migration nobody dares run, so a test walks all
   the way down and back up again and checks what survives. Undoing is for tests and for a release that has to go back,
   not for routine operation.

## What this does not decide

It does not make a migration safe to run while the platform is writing. A step that rewrites a table takes a lock, and a
deployment with heavy traffic still needs its own window; the platform runs the steps when it opens the store, which for
the engine is at startup. It also does not version the *record* shapes inside `props`: a record travels as the JSON it
arrived as, so a field added to a domain's record needs no migration and an old record simply lacks it.

## How this is held

Five tests on the decision itself (`migrations::plan`): an empty store runs every step, a store behind by one runs only
the step it is missing, a store at the current version runs nothing, a store written by a newer build is refused with a
message that says what to do, and a backend that cannot read a version row at all is treated as new. Three against a
real Postgres server: a fresh database reports the version this build knows and a record written before a second open is
still there afterwards; a database claiming schema 99 is refused rather than opened; and every step can be undone and
re-applied over records that already exist. One needs no server: both stores carry the same step names in the same order.
