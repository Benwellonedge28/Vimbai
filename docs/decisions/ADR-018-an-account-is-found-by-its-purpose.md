# ADR-018: An account is found by its purpose, not by its name

**Status:** accepted, 2026-11-25
**Depends on:** ADR-014 (nothing business-specific is code), ADR-015 (ledgers created only when needed)
**Amends:** ADR-014, item 1, which said account numbers and names are data set per Book. They were data, but the engine
still found them by their English name.

## The problem

Every journal needed to answer "which of this Book's accounts is the sales account?", and the answer was a substring
match on the account's name. `account_named` in `rust/crates/domains/accounting/src/subsidiary_journals.rs` took a word
like `sales`, `receivable`, `payable`, `inventory`, `accumulated depreciation` and matched it, case-insensitively, inside
the account's name, restricted to an account type.

That works for a Book whose chart uses the words the engine guessed, and fails for every other Book. A business that calls
its sales account "Turnover" is told it has no account for sales. A Book kept in another language cannot post a sale at
all. Renaming an account in the chart silently broke every journal that had been finding it by name - the tax ledger's own
test had to rename a bank account to "bank nowhere" to make a point, which is exactly the fragility this ADR removes.

The name is the business's word. It must never be the engine's key.

## The decision

1. **An account carries a `role`.** `StandardAccount` in `chart.rs` gains `role: &'static str`, seeded onto the account
   record with the rest of the opening chart, and kept by `ledger_book::account_body`, so renaming an account never loses
   the purpose it is kept for.

2. **The engine posts by role.** `account_named` finds the account whose `role` equals the purpose asked for, within the
   account type. Call sites pass a role identifier (`sales`, `receivable`, `payable`, `depreciation_expense`,
   `doubtful_allowance`, ...) rather than an English phrase.

3. **The roles are the laws of bookkeeping, not a trade's vocabulary.** `ledger_book::ROLES` lists the twenty-five: what is
   owed to the business, what it owes, what it sold, what came back, what it holds, what it has used up, what it keeps for
   somebody else, the money it keeps. Every set of accounts has these; what they are called is the Book's business.

4. **A purpose belongs to one account.** `update_account` refuses a role the engine does not post by, naming the ones that
   exist, and refuses a role another account already carries, naming that account - two accounts keeping sales would post
   sales twice.

5. **An older Book keeps working.** A Book opened before roles existed has accounts with no role, and `account_named` still
   answers by name for those accounts only (the role word with underscores read as spaces). No Book is made to
   reconfigure itself to keep posting; setting a role moves that account onto the purpose, after which its name no longer
   matters.

## What this forbids

* Any new lookup of an account by an English word in an engine path. A journal that cannot find its account by role must
  refuse in words and say which purpose is unset, not guess by name.
* A role invented for one business. If a journal needs a new purpose, the purpose is added to `ROLES` and the opening
  charts, because it is a law of accounting that every Book shares.
* Two accounts holding one purpose.

## How it was proved

`chart::tests::a_book_that_renames_its_sales_account_still_sells_through_it` renames 4000 to "Turnover" and posts a sale,
then reads the entry back to find the credit on 4000.
`a_book_opened_before_roles_existed_still_answers_by_name` builds a chart with no role at all and posts a sale through it.
`a_purpose_belongs_to_one_account_and_cannot_be_claimed_twice` proves both refusals. 458 accounting tests and 85 route
test binaries pass.
