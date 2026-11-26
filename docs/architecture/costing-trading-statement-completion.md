# Work package 91 — Complete a costing statement without appending expenses twice

Costing-statement source review [5c2d0018](https://github.com/Benwellonedge28/Vimbai/commit/5c2d0018ed10d212ab43eda94a0a8d2509c3ea0d).

## The mutable section

`add_statement_expenses` in `rust/crates/domains/costing/src/statement.rs` reads existing display lines, appends the profit and expenses section, and overwrites the statement. A retry appends a second section. It can interpret an unreadable gross profit as zero. This package concerns `TRADING_ACCOUNT_STATEMENT` in the costing service, separate from the [accounting trading-account evidence](saved-trading-account-evidence.md).

## Complete one deterministic revision

Store the base gross-profit calculation as immutable input for the verified completion path. Require a scoped statement ID, expected revision, operation key and all three exact expense values at the statement's currency precision. Use positive expenses or explicit zero. Reject an unreadable base rather than assuming a result. Build the completed line list from the base section and one freshly calculated expenses section. Net profit is gross less distribution, administration and other expenses. Freeze the input base revision, expense evidence, actor and exact totals.

Insert the completed revision and advance the current pointer under a scoped transaction. An exact replay returns its original revision. A changed input requires a new amendment with reason and expected latest revision. Do not edit the prior display rows. Completing a costing statement never alters accounting journals, close status or ledger reports.

## Consumers and history

Have the adapter invoking `statement::add_statement_expenses` in `rust/apps/api-server/src/records.rs` accept the versioned key and expose the prior and completed revisions. Preserve existing response keys for older versions. For historical rows with repeated expense sections, retain the original and show that it needs review. A verified amendment rebuilds from a reviewed base and explicit expenses, without guessing which appended section mattered.

Say “This statement changed. Open its current revision” for a stale edit and “The original gross profit could not be verified” for an unreadable base.

## Verification

- In the statement module add `replayed_statement_completion_contains_one_expense_section` and `corrupt_base_gross_profit_cannot_complete_as_zero`.
- In `rust/apps/api-server/tests/records_http.rs`, add `costing_statement_amendment_keeps_the_original_completion_readable` and cover another Book's statement ID.
- Add `concurrent_costing_statement_completions_leave_one_current_revision` in the provider target proposed in [package 47](subsidiary-journal-posting-receipts.md).

Done when the report shows one expenses section per revision and can reproduce each historical completion.
