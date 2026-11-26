# Work package 50 — Tie a recorded cash share issuance to its journal

Proposal pinned to [2c0642c](https://github.com/Benwellonedge28/Vimbai/commit/2c0642cd994674e4d455e93d524031eb6477e9a1).

## What the existing capital register returns

`record_capital_transaction` in `rust/crates/domains/accounting/src/company_accounting.rs` writes a `CAPITAL_TRANSACTION` before looking up the optional company. For `share_issuance` it calculates the nominal value as `price_per_share × number_of_shares` and returns suggested journal lines. The account codes are built from the company code. Neither the function nor its adapter in `rust/apps/api-server/src/records.rs` posts the journal. The source row starts with a null journal link. `list_capital_transactions` is the existing capital register. The register also accepts things outside cash issuance, so the new command must be deliberately narrower.

## Record funds already received for an issuance

Add a posted-issuance command under the company-accounting route family. It records an external receipt already confirmed by an authorized Book actor. Require company ID, configured class, issue effective date, a positive integral share count, positive nominal price per share, nonnegative premium, positive gross cash received, source evidence reference, and the receiving bank account. Scope the company, class, account selections and evidence to the active Book. Resolve the class from its company’s share-capital rows, selecting the latest approved effective record on or before the issue date. Refuse equally dated conflicting records and save the selected class row ID. Use exact Book-currency `Money` and check gross equals nominal value plus premium. Keep the legacy meaning of `price_per_share` as nominal price on the capital row and store the calculated nominal total with the receipt. Paid-in amounts and share quantities use different types.

Resolve a Book-owned mapping for the company and class to the eligible equity account and, when premium is nonzero, the eligible premium account. Record the versions and account IDs used. The same account may not play both roles for one issuance. Require the selected bank asset from the Book's chart and use the existing account resolver in `subsidiary_journals.rs` to validate explicit chart selections. Debit the bank for gross, credit the issued-capital account for nominal value and credit the premium account only when premium is positive. Require an open posting date. This command adds the capital transaction and linked journal. Holder allotment and issued-share register changes require separate reviewed commands and remain outside this release.

Use the [package 22](opening-balance-receipts.md) transaction primitive. Reserve `(Book scope, company ID, issuance reference)` with a versioned digest of reviewed input fields and create the posting through `ledger_book::create_entry`. Store the bank evidence reference, full journal ID, source row ID and actor in the same commit. An exact replay returns the original IDs even after policy changes. A different request with the same reference returns a conflict. A ledger correction uses an authorized reversal whose receipt links to the original issuance and journal. The capital listing must keep the original record available alongside its correction.

## Public surface and historical evidence

Add `POST /company-accounting/capital-transactions/posted-issuance` and its records alias in `records.rs`. Register both through the existing API inventory. Return `transaction` and the actual `journal_entry`, including its ID. Keep the older route's proposal and register behavior during the transition. Before a new posted issuance, check the complete company register for the supplied reference. If an earlier row uses it, direct the caller to the review path. Serialize that check with legacy issuance writes. An attempt to reuse a reference already reserved by a posted issuance is a conflict on either path. There, attach a journal ID only if the reviewer confirms a unique, unreversed, same-Book posting with the exact date, bank, capital, premium lines and cash evidence. Earlier capital rows and suggested lines remain available for review. Ambiguous records stay unlinked, and review does not post again.

Show “The cash received must equal nominal capital plus share premium” for a mismatch, “Choose this company’s issued-capital and premium accounts, then retry” for missing mappings, and the existing refusal for a closed date. An already used reference says “This company already has an issuance with this reference. Open the original record.” On a failed commit keep the form inputs and say “The issuance and its journal could not be saved.”

## Release checks

- In the company-accounting module, add `issuance_uses_book_account_ids_and_exact_nominal_plus_premium`. Cover a renamed chart, an exact large decimal, zero premium, and a one-unit mismatch at the stored precision.
- In the accounting provider integration target defined in [package 47](subsidiary-journal-posting-receipts.md), add `issuance_receipt_and_ledger_commit_or_rollback_together`. Inject a record write failure and race two identical submissions on each provider.
- In `rust/apps/api-server/tests/records_http.rs`, add `posted_issuance_replays_on_records_alias_and_rejects_another_book_company`. Assert the date refusal and the original source and journal IDs after a successful replay.
- Add `previous_capital_reference_requires_review_before_posting` to the company-accounting tests. Include an earlier capital row with two candidate journals and verify the ledger count does not rise.

The implementation is ready when the capital register can open the actual journal for every new posted issuance, with the evidence and exact amounts recoverable from storage.
