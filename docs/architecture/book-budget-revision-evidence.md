# Work package 62 — Keep spending-plan revisions available for comparison

Source baseline [ca0a15a4](https://github.com/Benwellonedge28/Vimbai/commit/ca0a15a467b3263bf2ae5a5342ee30ca2120252e).

## Why a saved plan can lose its evidence

In `rust/crates/domains/budgeting/src/budget_register.rs`, `remove_budget` deletes the record and its embedded lines. `lines_of` treats invalid serialized line JSON as an empty array. In `rust/apps/api-server/src/records.rs`, `plan_variance` loads the plan to obtain its reporting dates, reads `ledger_book::posted_activity`, then calls `budget_register::variance`, which reads the plan again.

## Immutable revisions and live actuals

Keep `BOOK_BUDGET` as the owner. Add a versioned save action with a stable plan ID, revision ID, prior revision ID, authenticated actor, Book currency and exact account amounts. A revision inherits the plan's reporting window. A plan for another window receives a new plan ID. Require actual calendar dates, a nonempty plan, one eligible Book account per line and positive budget figures. Store each stable chart identity and the account label and number used at preparation. Match actuals through the selected Book's chart identity. The current `posted_activity` returns a map keyed by account number. Add an identity-based reader in `rust/crates/domains/accounting/src/ledger_book.rs` over the same posted lines and normal-balance rules. Keep the existing number-keyed reader for other callers. Where a posted line has only a historical number, use verified chart history and report ambiguous mappings. A revision must never acquire activity from another account because a number was reused.

Reserve the submitted revision key per Book and actor and require the observed prior revision when amending. Save and advance the current pointer in one transaction. Matching retries replay the receipt. Archived plans remain readable by revision ID, and disappear from the default active list. The existing DELETE action archives versioned plans while preserving its success response. Use a scoped stable cursor for the history. Budget changes have no effect on posted balances.

When reporting a variance, load one saved revision allowed by Book access policy and take its date window into `ledger_book::posted_activity`. Pass that same revision into the budgeting comparison without reloading it. Keep the posted activity reader as the authority for actuals. Capture the revision ID and ledger read watermark together so the screen can describe the comparison. Missing activity is zero only when a complete read succeeded for a resolved account. For corrupted input say “This plan’s saved lines could not be read. Review the original plan.” On a competing revision say “The plan changed while you were editing it. Open the latest version.”

## Callers and older plans

Extend the existing `/records/budget-register/budgets` family in `records.rs` with separate revision creation and lookup actions. `apps/flutter/lib/features/budgeting/data/spending_plan_service.dart` should retain selected revision IDs. `spending_plan_page.dart` should explain preserved history after archive and keep the selected reference when reopening a comparison. Keep unversioned plan IDs readable. Migrate a sound legacy plan as its first historical revision, preserving original ID and dates. Retain unreadable lines for review instead of storing an empty replacement.

## Tests

- Add `malformed_plan_lines_do_not_read_as_zero_budget` to the budget register unit tests.
- Add `plan_variance_reads_window_and_lines_from_one_revision` to `rust/apps/api-server/tests/records_http.rs`, covering an intervening revision, another Book and an account-number rename.
- Add `budget_revision_replay_and_archive_preserve_historical_plan` to the provider tests from [package 47](subsidiary-journal-posting-receipts.md), racing two amendments.

Accept when a comparison identifies the exact plan it used and an archived revision still gives the original planned figures.
