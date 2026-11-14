# The scheduled-automation worker

`rust/apps/worker` is the process that runs a rule the clock was due to start. `domains/automation` could already hold a
rule, walk its steps and answer its history; nothing ran it. This does.

```
VIMBAI_STORE_FILE=/var/lib/vimbai/store.json vimbai-worker
VIMBAI_STORE_FILE=... VIMBAI_WORKER_ONCE=1 vimbai-worker   # a single pass, for a test or a cron job
```

Configuration: `VIMBAI_STORE_FILE` (the file the on-device runtime writes, chosen before a database), or
`VIMBAI_NEO4J_URL` with `VIMBAI_NEO4J_USER` and `VIMBAI_NEO4J_PASSWORD`. `VIMBAI_WORKER_INTERVAL_SECONDS` sets the pause
between passes (default 60). A worker with no store says so and exits rather than looking healthy.

## Running a rule exactly once

A worker can be stopped between claiming a run and recording that it finished: a redeploy, a full disk, a container that
is simply killed. If a restart then started the run again, a rule that posts a journal entry would post it twice, and the
second posting is indistinguishable from the first.

So a run's identity is the rule and the instant it was due, `run-{rule_id}-{due_at}`, and claiming means writing the run
row under that id:

1. A second worker cannot write the id, so it cannot start a second run.
2. A worker that restarts finds the row it already wrote, marked `running`, and finishes *that* run instead of opening
   another. This is the crash case, and it is the reason the row is written before the work rather than after it.
3. A completed run is never repeated, and completing advances the rule's due instant past now, so one due instant cannot
   fire twice even once the row is in the past.

The library takes `now` as a parameter everywhere. A scheduler that reads the clock cannot be tested, because "a rule due
later does not fire" would have to wait until later.

## What a rule carries

A rule's `condition` is a free-form object; the schedule lives in it under `schedule`:

```json
{ "schedule": { "due_at": 1763212800, "repeat_seconds": 3600 } }
```

`due_at` is the next instant the rule is due, in seconds since the epoch - the same reading `common::clock::moment()`
returns and the same unit the services already stamp, so a rule is compared against the store without a date parser in the
way. `repeat_seconds` is optional: without it the rule fires once; with it, the worker advances the instant by whole
periods until it is in the future, so a worker that was down for three hours does not replay three missed hours one after
another.

Only `trigger: "scheduled"` rules that are enabled are the worker's to start. A rule a person starts by hand stays manual.

## What a run leaves behind

A `WorkflowExecution` row under `/automation/executions`, carrying the steps that were taken, the status, when it
finished, and why it stopped when it did. A completed or failed run also lands in the owner's inbox through the
notifications crate, so a rule nobody is watching still tells somebody what it did.

## Tests

- `src/lib.rs`: eleven tests over the rules - due now fires once, due later does not fire, a crash between claim and
  completion leaves the run claimable exactly once, a completed run is not repeated, a repeating rule advances its due
  instant and skips missed periods, a manual rule is left alone, a disabled rule is not run, a failing step is recorded as
  a failed run and still notified, a run lands in the inbox, and one tick works every Book in the store.
- `tests/offline.rs`: five tests against the real binary and a real store file - a pass runs a due rule and a second pass
  does not repeat it, the run outlives the process, an idle store is left alone, an unconfigured worker says so, and the
  same rule and instant name one run across processes.
