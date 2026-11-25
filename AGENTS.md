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

## Writing a design document

A design document is read by someone who has to build from it, or by someone deciding whether to. Both need the same
thing: specifics they can check. A collection of thin pages that restates the architecture in other words helps neither,
and it is worse than no document, because it looks like the work is specified when it is not.

One document per work package. Claim it as a draft pull request first, like any other task, and say in the body which
work package it specifies.

A document that is ready to build from answers all of these, in its own words, about its own subject:

1. The real files and symbols it touches, as paths that exist at the pinned commit, and what changes in each. If it
   proposes a new file, it says where and why nothing existing can hold the change.
2. The invariants that must still hold afterwards: what stays true about the ledger, the scope, the money, the periods.
3. The errors, and what the person sees for each one, in words rather than an error code.
4. The migration for data already stored, or a sentence saying there is none and why.
5. The callers that must change with it, named.
6. The tests that prove it, named as they will be written, and what each one shows that the others do not.
7. The done criteria somebody else can check without asking the author.

Rules that have caught real problems here:

- Pin the commit the document was written against, and do not state a fact the document has not read. An owner, a route
  or a count is either verified at that commit or it is not claimed. "Not yet audited" cannot sit beside an assertion
  that reads like a finding.
- Do not repeat a sentence or a paragraph across documents. Boilerplate is the signature of a template being filled in
  rather than a subject being thought about; a shared requirement belongs in one place that the others link to.
- Measured numbers or none. No capacity claim without the hardware and the workload beside it, no acceptance criterion
  that cannot fail.
- If the design contradicts an accepted decision in `docs/decisions/`, propose the amendment in the same pull request and
  say which ADR it changes. Never quietly design past one.
- One place for architecture. Extend `docs/architecture/` and `docs/decisions/`; a new document tree is linked from
  `docs/README.md` in the same change, and says what it replaces or that it replaces nothing.
- Out of scope: anything where Vimbai moves money. It records what a person reports happened in accounts they already
  hold, and it does not hold funds, settle, pay out or run a wallet; Vimbai Pay stays not built (ADR-005) and the
  developer platform stays deferred (ADR-006). A document proposing that Vimbai hold or move a customer's money is
  closed, not merged, unless it first amends those decisions.
- In scope, and to be designed properly when their phase arrives: how Vimbai is paid for itself, and how referrals and
  partner commissions are recorded. Plans, price versions, trials, entitlements, past due and cancellation, a referral
  code and an award, a commission owed to a partner - none of these move money and none of them is Vimbai Pay. They are
  commercial records, they live outside every customer's Book, and a subscription or a referral award never appears in
  anybody's ledger. Charging for Vimbai is not Vimbai Pay, and a design that confuses the two has misread ADR-005.

The test of a finished document is blunt: another agent can implement it without asking a question. If it cannot, it is
an outline, and an outline is a fine thing to write in a pull request description, not in `docs/`.

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

## Dependencies

Dependabot keeps reporting every bump, and its pull requests are read in a weekly pass. Nothing is silenced: no
`open-pull-requests-limit: 0`, no blanket `ignore`, because a bot that stops talking is how a library quietly rots.

What the pass does instead of muting:

- Only ecosystems that still apply are configured. `pip` was removed with the Python it watched; do not add it back
  while the repository holds no Python.
- Each remaining ecosystem groups its bumps into one pull request a week, so a week's reading is three pull requests
  rather than a dozen, and every bump is still named.
- A bump that is a deliberate upgrade rather than a routine one - a base image, a crypto or JWT library - is declined
  with a comment and tracked in an issue, so the reason survives the closed pull request. Merging it later means
  running the gateway and library tests, not trusting the bot's green tick.
