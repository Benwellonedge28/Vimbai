# ADR-016 — A rule has hands, and a Book starts with its clock work done

**Status:** accepted, 2026-11-25
**Supersedes:** nothing. Amends nothing. It closes the gap that ADR-013 left open.

## The problem

ADR-013 says Vimbai is autonomous: once a fact is recorded the rest of the pipeline runs to the reports, and nobody
triggers anything. The worker honoured the clock, but not the work. `walk_steps` recorded each step as done and called
nothing, so a rule that said "charge depreciation" produced a run marked completed and a Book whose depreciation was
never charged. That is worse than not being automatic: it is a system that reports success for work it did not do.

Two things were missing, and they are separate:

1. **Hands.** A step named an action, and no code turned the action into the engine's own work.
2. **A starting state.** Even with hands, a new Book had no rules, so "automatic" was a promise the owner had to keep
   by building one.

## The decision

**A step reaches the engine.** `rust/crates/application/automation-hands` is the one place a rule's step becomes an act. It offers
`post_depreciation` (write off one month for every asset the Book still holds) and `close_period` (close the month).
Both call the same domain code the routes call, so a rule run by the clock and the same act taken by a person cannot
diverge.

**A step nothing can do stops the run, in words.** An unknown action fails the step with a sentence naming the actions
that do exist. It is not skipped, and it is not recorded as done. A run that cannot finish is a run the owner hears
about (the worker already notifies on failure), and a wrong name is then a five-second fix rather than a silent month
of missing postings.

**Both acts are safe to repeat.** The ledger refuses a month already charged, and closing a closed month answers
"already closed" rather than failing. So a run resumed after a crash, or claimed twice, finishes what is left without
charging anything twice. Idempotency lives in the ledger, not in the worker's bookkeeping, because the ledger is the
thing that must not double-count.

**A month is a calendar month.** A rule may repeat `every: "month"`, and the worker advances to the first instant of
the next month. Thirty-day arithmetic drifts: after a year of 2,592,000-second "months", a month-end rule charges
November in December. A repeat the worker does not understand is refused by name rather than approximated.

**"last_month" is a period a step may ask for.** A rule firing on the first of December is charging November. Saying
`"period": "last_month"` keeps that fact in the rule instead of hiding it in arithmetic.

**Nothing to charge is a state, not a fault.** On a Book holding no asset, the depreciation step answers
`charged: 0` with a sentence, and the run completes. A Book that has never bought anything should not fail its
month-end rule every month and teach its owner to ignore notifications.

**A Book is opened with its rules.** `POST /books` installs them, and `POST /automation/standard-rules` gives them to
a Book that predates this or lost them. Installing is safe to repeat: a rule the Book already has, by name, is left
exactly as the person left it. A person who switched the depreciation rule off does not find it switched back on.

## What ships on, and what ships off

Two rules ship, and they are deliberately not the same kind of thing.

**Charge depreciation every month — on.** It cannot do harm: it answers zero on an empty register, it charges the
month that just ended, and it is work a person would otherwise have to remember twelve times a year.

**Close the month — off.** Whether a clean month closes itself is a decision about the owner's own books, not a
technical default. It has not been decided. A rule that states what it would do and waits to be switched on is the
honest way to ship a decision nobody has made. When the owner answers, the answer is one row of data and one switch.

## What this does not decide

- **Which facts post themselves.** Depreciation and closing are time-driven, and time is unambiguous. Accruals, ageing
  and provisions need a policy the owner sets; when they arrive they arrive as more actions in the same place, not as
  a second mechanism.
- **Whether the phone keeps posting buttons.** ADR-013 makes recording a fact the normal path. Removing the buttons is
  a separate act and still the owner's call.
- **Whether owners write their own measures and profiles.** Unchanged and unanswered.

## How it is proved

`crates/application/automation-hands` (8 action tests): depreciation writes off every held asset, a repeated month is refused
by the ledger, a month before acquisition is refused in words, closing a closed month is answered not repeated, an
empty register answers zero, `last_month` resolves to the month before, an unknown action names the ones that work.
`crates/application/automation-hands/src/standard.rs` (3 tests): a new Book gets both rules with the right switches and the right
due instant, installing twice adds nothing and never overwrites a person's change, and a second Book sees none of the
first's. `apps/worker` (13 + 1 + 5 tests): a monthly rule lands on the first of the next month and not thirty days
later, an unknown repeat is refused, and the timing tests now name work the engine can do. `apps/api-server`
(`automation_engine_http`, `book_sync_http`): a rule over the real routes charges depreciation and closes the month, a step nothing can
do fails the run naming what exists. `book_sync_http`: a Book that has just been opened already has its clock work, and asking again adds nothing.
