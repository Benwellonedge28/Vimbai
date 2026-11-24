# Runbook: when a deployment is not answering

This is the page to read while something is broken. It is not a description of the architecture (that is
`docs/architecture/overview.md`) and not a setup guide (`docs/LOCAL-RUNTIME.md`). It answers three questions in order: is
anything running, is the store reachable, and which layer is refusing.

Every setting named here is checked against the settings the source actually reads, by `xtask::runbook`: a name nothing
reads fails the build, so this page cannot quietly drift from the code.

## 1. Is anything running

The engine prints one line when it starts, and it is the line to look for first:

```
vimbai-api listening on 0.0.0.0:8080 (workers 8, at most 256 requests at once, 30000ms deadline)
```

If that line is absent, the engine never bound its address.

*A bind error.* `panic!("cannot bind {address}: …")` means the address is taken or the process may not have it. The
address comes from `VIMBAI_API_ADDR` (default in the manifests, `0.0.0.0:0` when unset, which picks a free port and
prints it - useful for a test, not for a deployment where anything has to find it). Check what holds the port, and check
that the container is allowed to bind it.

*No store.* The worker says `no store configured: …` and exits, deliberately: a worker beside a database it cannot reach
would sit there looking healthy while nobody's scheduled rules ran. The engine and the worker both ask one function which
store to open, so if one of them reports this, both will.

## 2. Is the store reachable

Two settings choose the store, and the order is the deployment decision (ADR-007), made in one place:

`VIMBAI_POSTGRES_URL` is the system of record, and the only deployment store. A host without it refuses to start rather
than falling back to whatever is installed, and says which setting it wants.

`NEO4J_URI` (with `NEO4J_USER`, `NEO4J_PASSWORD`, `NEO4J_DATABASE` when the server needs them) is read by
`vimbai-store-move` only: a business still running Neo4j lifts its books out with `vimbai-store-move neo4j
postgres:<dsn>` and then points the engine at the new database. Neo4j is never a destination.

`VIMBAI_STORE_FILE` is the on-device store, for the runtime that keeps a machine's records in a file. It is not a fallback
for a deployment: a machine with neither a database nor a file is told so, rather than given an empty store that looks
like data loss.

`VIMBAI_POSTGRES_POOL` sets the connection pool size (four when unset). A pool exhausted by long requests looks like
"everything is slow" rather than like a database problem, so a deployment under load should raise it before it raises
anything else.

To check the store without starting the engine, move it into a file and compare:

```
cargo run -p vimbai-store-move -- postgres:<dsn> file:/tmp/check.json
```

It reads every record kind and every edge, writes them into the destination and verifies what arrived, exiting non-zero on
anything it cannot prove - so a successful run is both a reachability test and a schema check, and the file it leaves is
small enough to look at. The schemes are `file:<path>`, `sqlite:<path>`, `postgres:<dsn>` and `neo4j`.

### The migration refusal

A store whose schema is newer than the build gets this, from either backend, worded once:

```
this store was written by a newer build: schema 12, this build knows 9. Upgrade the engine rather than opening it with
an older one, which would write the next step against a schema it cannot see
```

That is not a problem to work around. A database at step 12 has had steps this build does not know about; running this
build's steps would record a version below what the database actually is, and the next migration would be written against a
shape it never expected. **Upgrade the engine.** If a rollback is genuinely required, it has to be done with a build that
knows the schema, and the reasons are in ADR-011.

The reverse case is routine and needs nothing from anybody: a store behind the build has its missing steps applied when the
store is opened, each step in a transaction with the version row, so a step that fails half way leaves the previous
version rather than a half-built schema.

## 3. Which layer refused

Once something is running, the status code says who is talking (`crates/adapters/gateway` is the shared rule, used by the
on-device runtime and the sync server as well as the cluster's Go gateway):

`401` with `Not authenticated` - no `Authorization: Bearer …` header at all. `401` with `Invalid or expired token` - a
token was sent and did not verify. The identity door and `/health` are open on purpose: a caller cannot hold a token before
they sign in.

`403` with `No active membership in book …` - the caller is known and may not open that Book. A member of the organisation
that owns the Book gets in without a per-Book row; anyone else is refused rather than shown an empty listing.

`422` from a write that carried an idempotency key already used with a different body - the key belongs to another
request. `409` from the same key while the first attempt is still running - the caller should wait rather than send again.
A repeat with the same body replays the first answer, including a refusal.

`503` with `the engine is at its limit: …` - the door shed the request because too many were already in flight. That is the
door doing its job: it protects what is behind it from a queue that would only make everything slower. `504` with `the
request did not finish within …ms` - one request held a slot past the deadline. Both are tunable, and both are per-process:
`VIMBAI_API_MAX_IN_FLIGHT`, `VIMBAI_API_TIMEOUT_MS`, `VIMBAI_API_WORKERS`.

A refusal is not the same as a fault: `401`, `403`, `409`, `422`, `503` and `504` are the platform telling a caller what
happened. A `500` is the platform failing.

## 4. Scheduled rules are not firing

The worker claims a rule, runs it once, and records the run against the instant the rule was due, so a crash between claim
and completion leaves the run to be finished rather than started again. When nothing fires, check in this order: is the
worker running at all; does it open the same store the engine does (see above - this is the failure that has happened
before); is the rule enabled and due.

`VIMBAI_WORKER_INTERVAL_SECONDS` is the pause between passes (sixty when unset) and `VIMBAI_WORKER_ONCE` set to `1` runs a
single pass and exits, which is what a cron job and a test both want. A manual run of the worker with that setting is the
cheapest way to see what it would do now.

## 5. Where the settings live

In a deployment, in the manifests under `infrastructure/`. On the cluster, the overlays in `infrastructure/kubernetes/`
that carry the values for an environment. On a machine, `docs/LOCAL-RUNTIME.md` explains the data directory and the one
setting that matters there: `VIMBAI_LOCAL_PROFILE`, with `VIMBAI_LOCAL_MEMBERS` naming which services a profile loads.

A secret (`VIMBAI_POSTGRES_URL` when it carries a password, `VIMBAI_JWT_SECRET`) comes from the secret store of whatever
runs the manifests, never from the file itself.
