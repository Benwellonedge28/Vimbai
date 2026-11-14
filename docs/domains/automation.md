# Automation

The rules a Book runs by itself: what starts a rule, the steps it takes in order, what happens when a step cannot go ahead, and the record of every run.

Crate: `rust/crates/domains/automation`

## What it owns

A rule has a trigger - scheduled, entity, connector or webhook - a condition, and a list of steps. Walking the steps is the interesting part: a step that depends on another runs only once that one has succeeded, and a step that cannot go ahead stops the run naming the dependency it was waiting for, rather than continuing with half the work done.

The run history is the other half. Every run is a record with its steps, its status and its reason, so a rule that quietly stopped working is visible instead of merely absent.

Scheduled rules are started by the worker process (`apps/worker`), not by this crate: the worker decides which rule the clock is due to start and runs it exactly once, which is a different problem from walking the steps (see `docs/WORKER.md`).

## Services and addresses

| Service | Prefix |
| --- | --- |
| `automation-engine-service` | `/automation` |

## What it does not own

The clock, and the exactly-once guarantee. A domain crate has no business waking itself up; that belongs to the worker process. Nor does this crate decide what a step means: a step that posts entries asks the accounting crate to post them.

## Tests

The automation suite walks a definition from its first step to its last, proves a signature step refuses to close before the check step, and covers the runs listing and the run-by-rule door. The worker's own tests prove the schedule contract end to end against the real binary.
