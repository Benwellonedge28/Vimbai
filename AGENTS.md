# Working in this repository as an agent

More than one agent writes here: a person, and more than one AI. These rules exist because two agents working the same
task at the same time produce two conflicting changes and no way to tell which one was meant. Nothing here is about
trust. It is about one task having one owner at any moment.

## One task, one agent

Read the open pull requests, the branches, and `docs/ROADMAP.md` before you start anything. If a task is already
claimed, do not start it. Pick something else, or say in the pull request that you think it is being done wrongly.

Claim a task as the first thing you do, before writing code: open a draft pull request. The branch name says who you are
(`codex/…` or `vimbai/…`), the title says the task in one line, and the body says what you are about to change. The draft
pull request is the claim. Opening it early costs nothing and tells everyone what is taken.

Release the claim by merging it, or by closing it with a sentence saying why you stopped.

## Do not work the same thing twice

Not the same feature, not the same file, not the same long command. Do not run `./scripts/preflight.sh` on a branch that
is not yours, and do not push to a branch another agent is working on. If you are unsure whether a task is taken, ask in
the pull request and wait; a few minutes of waiting is cheaper than a duplicated change.

## Never push to main

`main` is protected: no deletion, no force push, and no change that arrives without a pull request whose three required
checks have reported. The repository admin role bypasses that, which is for a person who means it, not for an agent.

## Handover

When a task passes from one agent to another, the receiver's draft pull request is the claim and the giver stops, closing
its own branch with a note saying what state it was left in. Both never continue.

## Say who you are

An AI agent names itself in the pull request body, so the record shows who made a change. A commit message explains the
change and why; it does not need to explain the agent.

## Before you push

`./scripts/preflight.sh` must be clean. The rules that are not waived for convenience:

1. Safe Rust only. `#![forbid(unsafe_code)]` at every crate root, including integration test crates.
2. No Python anywhere in the repository.
3. Financial arithmetic goes through `Money`. A float in a financial path is a bug.
4. Nothing posts to the ledger except through `crates/kernel/ledger-core`. Entries are immutable; a correction is a
   reversing entry.
5. Nothing business-specific is hard coded: account numbers, names, currencies, tax names and rates, calendars, industry
   lists, measures and labels are data set per Book.
6. Database neutrality is verified, not assumed: every store passes the one conformance suite, and a provider swap is an
   explicit act with a receipt.

Work follows `docs/ROADMAP.md` in phase order, and a change that belongs to a later phase is usually better landed when
its phase arrives. Read `docs/decisions/` before changing something an ADR explains.

Stay inside the task. Do not reformat or refactor files the task does not need; a diff that is hard to read is a diff
nobody reviews.
