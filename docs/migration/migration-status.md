---
title: Migration status
summary: The Python-to-Rust migration is complete; what remains is what was decided against, and why.
---

# Migration status: complete

The migration moved 141 Python service directories onto one Rust engine. It is finished: every routed service answers
from Rust, and the repository contains no Python at all - no `.py`, no `requirements.txt`, no `pytest.ini`, no
`conftest.py`, and no Python job in any workflow.

| | At the start | Now |
|---|---|---|
| Routed services | 138 | 138 |
| Served by Rust | 1 | 138 |
| Served by Python | 137 | 0 |
| Python service directories | 141 | 0 |
| Tracked Python files | 135 | 0 |
| Workspace tests | 2,191 | 2,817 |

## How it was done

Each service was retired only when the engine answered every address it published, by method, and the survey refused to
call a service ready otherwise. Three kinds of finding came up, and each has a durable gate now:

1. **Address mismatches.** The arithmetic was already in Rust but the engine answered `/x/thing` where the service
   published `/x-service/thing`. The fix is an alias in the engine, and `cargo run -p xtask -- gateway` fails if a
   retired service's published address is unanswered.
2. **Genuine gaps.** The arithmetic did not exist in Rust yet. It was written once, in the crate that owns the domain,
   with tests over the exact figures the Python service had answered.
3. **Wrong answers, not errors.** A service reading another service's handler returns plausible numbers. A stripped-path
   collision gate was added; every collision it found was closed, and its baseline is empty.

The duplicate-address gate came out of the same work: no HTTP address may be registered twice in the API server, so a
copy of a route cannot appear beside the original.

## What was decided against, permanently

These are not leftovers. Each is a decision with an ADR.

1. **Vimbai Pay was never built** (ADR-005). The platform records money; it does not move it. A deployment that needs to
   move money is a new decision, not a missing crate.
2. **The developer platform is deferred** (ADR-006), with its trigger written down: the first real request for API
   credentials.
3. **No Python client SDK.** The app is Flutter and the contracts are OpenAPI; a generated Python client would be a
   consumer nobody has.
4. **The PyO3 extension was deleted, not ported.** It existed to let Python call the Rust calculators. With no Python
   left there is nothing to call it.
5. **The kernel's reading half reads the store** (ADR-009). The one tolerated dependency, recorded rather than hidden.

## What the migration did not do

It moved the platform; it did not redesign it. Two things the design document describes are still ahead of the code, and
both are stated in the roadmap rather than implied here: the data layer (Neo4j, relational, or split by ownership - P8,
open) and attaching the 180 unattributed prefix groups to catalogue owners (P6.4 follow-up).

## Historical documents

`MIGRATION-MAP.md`, `MIGRATION-MANIFEST.md` and `RUST-MIGRATION-BACKLOG.md` are snapshots of the migration while it was
running. They are kept as history and are not maintained: where they disagree with this file, this file is right.
