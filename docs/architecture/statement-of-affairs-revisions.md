# Work package 52 — Preserve dated statements of affairs and their evidence

Proposal reviewed against [ca0a15a4](https://github.com/Benwellonedge28/Vimbai/commit/ca0a15a467b3263bf2ae5a5342ee30ca2120252e).

## Where older statements can disappear

In `rust/crates/domains/accounting/src/bookkeeping.rs`, `create_statement_of_affairs` saves itemized assets and liabilities and computes capital as assets less liabilities. It stores null for the statement's notes, accepts the as-of date as text, and treats a missing or malformed list as empty through `rows`. `statement_of_affairs_on` selects the first date match from a capped collection. Once enough other papers precede a saved statement, the lookup can say it is missing. The date also fails to identify a unique paper when there is more than one statement for that day.

## Save a dated working paper

Add an optional `calculation_version` field with the documented string value `"2"` to `create_statement` in `rust/apps/api-server/src/accounting_book.rs`. Preserve legacy behavior for unversioned submissions during the ADR-004 transition. The new request carries a stable opaque `working_paper_key`, an as-of date and explicit `assets` and `liabilities` arrays. An empty array means the preparer confirmed there are no lines on that side. Reject missing arrays, malformed lines, blank item names and negative balances. Each item carries an exact nonnegative amount in Book currency and an optional reference to its supporting evidence. An explicitly designated contra balance is deferred until the form can capture its type and evidence. Preserve itemization, notes, evidence references and the authenticated preparer. Use one shared version 2 amount validator for packages 52–56. The store’s `wire` in `rust/crates/adapters/database/src/amounts.rs` emits six decimal places. Validate each `Money` amount by an exact parse of its stored representation and reject any value that would change on serialization. Trailing zeros are harmless. Say “Enter an amount that fits the working paper’s precision” and identify the field or row. Never round a typed balance silently. Allow capital itself to be negative.

Store the date, currency, input lines, exact totals, metadata and optional `supersedes_id` in the existing `STATEMENT_OF_AFFAIRS` record kind. For an amended paper, require the predecessor to belong to the same Book and date. Allow historical records to be used as predecessors. In the same Book-scoped store transaction that inserts the revision, reserve the working-paper key and verify that the predecessor has no successor. Normalize effective dates and decimal values in the request digest and preserve line order. A matching retry reads the existing record by key. Reusing a key with different inputs or racing a second amendment returns a conflict. Separate working papers without a common predecessor may coexist on the same date. Saving or revising this evidence leaves posted balances and period locks untouched.

## Find and review

Add `GET /accounting/statements-of-affairs/by-id/:statement_id` and the equivalent `/records/accounting` alias. Use a Book-scoped `Records::get` and return the exact revision, including its predecessor. In `statement_on`, query by Book and exact date before applying result limits. Keep deterministic legacy selection by newest stored creation order and ID. Optional `revision_view=current` asks for the sole unreplaced record on that date and returns a conflict with the scoped candidate IDs if there are multiple chains. For `list_statements`, keep the legacy array and support a date-filtered, cursor-paginated view ordered by date, creation order and ID. Register the added routes in the existing API inventory.

Historical rows retain their IDs, unknown inputs and stored financial figures. Mark them as legacy and preserve their blank notes as recorded. There is no automatic backfill for omitted notes or unrecorded revisions. A preparer can supersede a historical paper after checking its source.

Show “Enter the date this statement describes and confirm both item lists” for missing date or array evidence, “This statement has already been revised. Open the latest revision” for a stale predecessor, and “Several statements exist for this date. Choose the statement you want to open” for an ambiguous current date lookup. An inaccessible ID returns the same generic refusal regardless of whether it exists elsewhere.

## Acceptance

- Add `statement_of_affairs_requires_explicit_sides_and_allows_negative_capital` beside the bookkeeping unit tests. Cover a missing array, a genuine empty side, a large exact amount, an amount beyond stored precision, and capital below zero.
- Add `statement_of_affairs_date_lookup_reaches_past_legacy_limit` and `statement_revision_remains_readable_by_id_after_amendment` to `rust/apps/api-server/tests/records_http.rs`. Assert the same behavior under both prefixes and check that another Book cannot read the ID.
- Add `statement_revision_has_one_successor_on_each_provider` to the integration target proposed in [package 47](subsidiary-journal-posting-receipts.md). Race two amendments and replay the winning key after restart.

Accept when a reviewer can recover a specific paper and its original inputs by ID, even after a revision and after its date falls beyond the former collection limit.
