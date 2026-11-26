# Work package 40 — Page Book search without losing older matches

Proposed design against `f9795238b56a8a02c8c7513cc72e484f69ff9b2d`.

## Existing path
`rust/crates/domains/accounting/src/find.rs::find` reads via `ledger_book::list_entries` and `ledger_book::accounts`, filters in memory and stops each result kind at `AT_MOST`. The response supplies counts and limits without a continuation. The adapter in `rust/apps/api-server/src/accounting_book.rs` invokes `find::find`.

## Query contract
Implement scoped search queries for entries and accounts. Apply matching before the page limit. Keep existing lowercase substring semantics and field precedence when assigning `matched_on`. Order entries by descending effective date and stable entry ID, and accounts by account number and stable ID. Keep separate cursors per result kind. Each cursor binds the Book, normalised query, kind, ordering boundary and snapshot identity. Keep the same read boundary across pages or return a restart response when it expires. Query filtering must behave identically on SQLite and PostgreSQL. Verify the full scope before evaluating a cursor.

Retain the current initial response fields. Add continuation and completeness for each kind. “No matches” is valid only after a complete search. An invalid or expired cursor tells the user “The Book search changed. Search again to refresh the results.” A failed store read reports an error. Neither condition can return another Book's metadata.

## Client and rollout
Update `apps/flutter/lib/features/find/data/find_service.dart` and `apps/flutter/lib/features/find/presentation/find_page.dart` to load and append the selected result kind, avoiding duplicates by ID. On a new query or Book switch discard old results and cursors. No record migration is needed. Search reads the existing journal and chart, including searchable drafts.

## Acceptance
- `find_returns_older_matches_beyond_the_initial_page` exercises a common term and a unique old reference.
- `find_cursor_rejects_wrong_book_query_and_kind` proves isolation and request binding.
- `find_pagination_keeps_stable_order_for_equal_dates` verifies provider parity and no duplicate hits.
- `find_ignores_a_late_response_after_book_switch` covers the client race.

Done when both result types can be exhausted with stable paging and the Book switch test shows no stale results.
