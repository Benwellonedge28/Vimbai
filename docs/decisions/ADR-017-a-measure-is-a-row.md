# ADR-017: A measure is a row, and six sources are all it is built from

**Status:** accepted, 2026-11-25
**Depends on:** ADR-014 (nothing business-specific is code), ADR-013 (a report is a view over any window)
**Amends:** ADR-014, item 3, which said a measure is a definition rather than a code path; this is that item built

## The problem

ADR-014 item 3 decided that a measure is composed from primitives the engine already holds, so that adding one is a row
of data. Until now the figures a Book watched were a single function: `progress` in
`rust/crates/domains/accounting/src/progress.rs` computed revenue, costs, profit, margin, cash, what was owed each way,
tax set aside and the count of unposted entries, and every one of those was a branch in that function. A business wanting
to watch its own line - spend on premises, a share of trading, the count of something - had no way to ask, and the only
way to give it one was another branch.

There is a second problem behind it. A measure has to be checked against the Book it belongs to, not against a list in
the engine. A Book that keeps accounts it calls something else must still be measurable, and a measure naming something
that Book does not keep must be refused in the Book's own words.

## The decision

1. **A measure is a record, one per measure, in the Book.** `MEASURE` is `RecordKind::new("BookMeasure",
   "OWNS_BOOK_MEASURE")` in `rust/crates/domains/accounting/src/measures.rs`. It carries a `name` (lowercase letters,
   digits, underscores), a `label` (what a screen shows), a `unit` (`money`, `percent`, `count`), a `source`, and the
   fields that source needs. Nothing about a measure lives anywhere but that row.

2. **Six sources, and no more.** `accounts`, `documents`, `tax`, `entries`, `difference`, `percent`. Each is a reader the
   engine already had: `accounts` reads `ledger_book::posted_activity` over the window, or `ledger_book::account_balance`
   as at its end when `window` is `as_of`, filtered by the row's `types`, `numbers` and `money_only`; `documents` reads
   `subsidiary_journals::receivables_ledger` or `payables_ledger`; `tax` reads `tax_ledger::tax_ledger`; `entries` counts
   `ledger_book::list_entries` by `status`; `difference` takes measures away from one another; `percent` divides one
   measure by another. A seventh source is a change to this record, not a new field.

3. **The Book's own chart is the vocabulary for accounts.** `chart_of` reads `ledger_book::accounts` and
   `cashbook::list_accounts`, so a row naming a type the Book does not keep is refused naming the types it does keep, and
   a row naming an account number it does not hold is refused naming the rule. No account number, currency, tax or
   industry appears in the module.

4. **Every Book opens with the measures every business reads, as rows.** `install_default_measures` writes nine:
   `revenue`, `costs`, `profit`, `margin`, `cash`, `owed_to_you`, `owed_by_you`, `tax_set_aside`, `not_posted`. They are
   rows like any other, so a person may relabel one, change what it counts or delete it, and installing again adds
   nothing. A Book is opened with them at the same point it is opened with its chart and its rules
   (`rust/apps/api-server/src/records.rs`), and `POST /accounting/measures/standard` gives them to a Book that predates
   this or whose owner took one away.

5. **A row that cannot be read says so beside itself.** `list_measures` works out the plain measures first and the
   derived ones second, so a difference or a share reads measures already worked out. A row naming a measure the Book does
   not have answers `value: null` with a `problem` sentence naming the measures the Book does have, rather than hiding the
   Book's other figures. A row whose accounts do not exist fails the whole answer, because that is a mistake in the row
   and not a reading.

6. **The window is asked for, never assumed.** `to` is required, because a reader that assumed today would answer
   differently each time it was asked; `from` defaults to the first of the month `to` falls in, which is the window a
   person standing in a month means. A day that is not a day, and a window that opens after it closes, are refused in
   words.

## Errors

| Situation | Answer |
| --- | --- |
| `to` missing | 422, naming `to=2026-10-20` and why the day is asked for |
| `to` or `from` not a day | 422, quoting what was sent |
| window opens after it closes | 422, naming both days |
| `source` not one of the six | 422, naming the six |
| `unit` not one of the three | 422, naming the three |
| account type the Book does not keep | 422, naming the types it does keep |
| account number the Book does not hold | 422, naming the rule |
| name already measured | 409, saying the Book already measures it |
| `of`/`over`/`minus` naming a measure the Book lacks | 422, naming the measures it has |
| deleting a measure the Book does not have | 422, saying so |
| a row naming a missing measure at read time | 200, that row `value: null` with a `problem` |

## Callers

`GET /accounting/measures`, `POST /accounting/measures`, `POST /accounting/measures/standard` and
`DELETE /accounting/measures/:name`, all in `rust/apps/api-server/src/accounting_book.rs`, and the Book-open path in
`rust/apps/api-server/src/records.rs`. `progress` is untouched and keeps its own shape: the two answer different
questions, and a phone that reads the decision view today keeps working while measures gain their screen.

## Tests

12 tests in `measures.rs`: the nine a Book starts with and installing twice adding nothing; revenue, costs and profit
reading the Book's own accounts; what is owed each way read from documents; a measure a person adds counting only the
accounts it names, and counting more once a rent entry is posted; cash reading the money accounts and not every asset;
a type the Book does not keep refused in its own words; an unknown source refused naming the six; a share of a measure the
Book lacks refused naming those it has; a measure added twice refused rather than shadowing the first; `not_posted`
counting drafts the statements leave out; a window that cannot be read refused; a measure taken away and then refused.
4 route tests in `rust/apps/api-server/tests/measures_http.rs` over a real Book: the figures answered, a measure added and
deleted, a type the Book does not keep refused, and no day refused rather than assumed.

## Done criteria

A Book answers the figures it measures itself by; a business adds a measure of its own without the engine changing; a row
naming something the Book does not keep is refused in the Book's own words; installing the defaults twice changes
nothing; and `./scripts/preflight.sh` is clean.
