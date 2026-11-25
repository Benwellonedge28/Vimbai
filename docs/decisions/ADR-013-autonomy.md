# ADR-013: A Book is autonomous - an entry flows to the reports, and a report is a view over any window

**Status:** accepted, 2026-11-25
**Depends on:** ADR-002 (ledger-core is the posting authority), ADR-012 (a Book is kept in one money)
**Amends:** nothing. It states what the engine already does and what no later work may undo.

## The problem

Every accounting system built by accretion ends up with a pipeline a person has to push: post the entry, then run the
depreciation, then post the accruals, then "generate" the report, then close the month, and only then is a figure
allowed to exist. Each push is a step where the truth can be stale, where a person can forget, and where the same
report answers differently depending on who ran what.

That is the wrong shape for a business of any size, and it is the wrong shape for Vimbai. A person sets the environment
up once - the chart, the taxes, the money, the calendar, the policies - and after that the system should behave as if
nobody were operating it.

## The decision

1. **The ledger is the truth and a report is a view.** A trial balance, an income statement, a balance sheet, a cash
   flow, a tax ledger, a general ledger and every measure built on them are projections of the posted entries over a
   window. No report owns a figure. No report has a "run" step.

2. **Every report answers over any window, ending any day.** A month is how figures are locked, not how they are seen.
   Nothing waits for a month to close before it can be read, and no reader refuses a window because the month is open.

3. **A person records facts; the engine posts.** What happened outside Vimbai - a bill arrived, a sale was made, money
   was paid, stock was counted - is what a person reports. Which accounts it moves is never a person's decision, and
   posting is never a button they must remember.

4. **Time-driven postings post themselves.** Depreciation, accrual release, the earned slice of a revenue contract,
   doubtful-debt provisioning, ageing and the month's lock are facts about the calendar. They post on the day they
   fall due, idempotently, keyed by a reference that names the period, so a second attempt is refused by the ledger
   rather than counted twice. No schedule is triggered by a person.

5. **Autonomy only ever moves forward.** Nothing the engine posts on its own may be a reversal of a fact a person
   recorded. A correction is a reversing entry made by the same authority that made the original.

6. **A missing policy stops the pipeline and says what to set.** The engine never guesses a rate, an account or a
   method. Where a policy is absent it refuses in words that name the setting - which is the opposite of failing
   quietly, and the only honest way to be automatic.

7. **What has not been posted travels with the figure.** A profit that leaves out unposted entries is provisional, and
   the reader says so in the same answer rather than leaving it to be discovered later. See
   `crates/domains/accounting/src/progress.rs`.

## What this forbids

- A report endpoint that needs a prior endpoint to have been called.
- A figure that exists only after a month closes.
- A screen whose normal path is a person choosing which account to post to.
- An automatic posting that cannot name why it happened, or that can run twice.

## What it does not decide

Whether a clean month closes itself is an open question for the owner, and so is whether the phone keeps a posting
button at all once recording facts is the normal path. Both are recorded as open in `docs/ROADMAP.md` rather than
decided here.

## How this is verified

`crates/domains/accounting/src/progress.rs` reads a Book to any day without a close and without a run step, and its
fifteen tests prove: the month so far answers before the month ends; a day before anything was posted reads nothing
rather than guessing; any week, quarter or arbitrary window answers over its own days; entries that are not posted are
counted and the answer says the profit is provisional; and the day is asked for rather than assumed, so two readings
can be compared.
