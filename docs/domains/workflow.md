# Workflow

The steps a process takes, in order, with the rule that a step does not close until the step it depends on has: approval chains, hand-offs, and the distributed orchestration that runs them across more than one worker.

Crate: `rust/crates/domains/workflow`

## What it owns

1. Workflow definitions and their runs: a definition is walked from its first task to its last, and a step that depends on another refuses to close before it - a signature step that closed before the check step would be a signature on an unchecked document.
2. Distributed orchestration: the same walk when the steps are handled by different workers, so the ordering rule holds beyond one process.

This is not the automation domain's job. A workflow is a process a person moves through; an automation is a rule the system runs by itself, and mixing the two is how a rule ends up waiting for a human who was never told they were needed.

## Services and addresses

| Service | Prefix |
| --- | --- |
| `distributed-workflow-orchestration-service` | `/distributed-workflow-orchestration` |
| `workflow-service` | `/workflow` |

## What it does not own

The clock. Nothing here wakes itself up; that belongs to the worker process, which reads scheduled rules and runs them exactly once (`docs/WORKER.md`). Nor does it perform a step's work: a step that posts an entry asks the accounting crate.

## Tests

The workflow suite walks a definition from first step to last and proves the ordering refusal, and the orchestration path is covered by the same rule applied across workers. The run history is tested to show every step, including the one that stopped it.
