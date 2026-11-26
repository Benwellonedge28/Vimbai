# Work package 67 — Reconcile a bank-feed line with verified journal evidence

Source baseline [3a2c2ca1](https://github.com/Benwellonedge28/Vimbai/commit/3a2c2ca16978df88a95ad6b1c1decb2148a8707b).

## Current write path

`reconcile_transaction` in `rust/apps/api-server/src/service_bank_feed.rs` saves the reconciled status before writing the optional journal link. If the link is absent or the second write fails, the first write can still stand. `reconcile_bank_transaction` in `rust/crates/domains/banking/src/integration.rs` writes another spelling of the match on the same feed record. `link_transaction` in `bank_feed.rs` copies references without reading the referenced journal.

## One verified match

Add a shared banking command that accepts a cleared feed line, expected line revision, posted journal ID, bank-account line identifier and a stable operation key. At the application boundary, resolve the journal through `ledger_book::get_entry` in `rust/crates/domains/accounting/src/ledger_book.rs` and the account through the connection's confirmed Book-owned chart mapping. For this release, support one feed line to one posted bank-account line in the same currency. Compare the signed cash-side amount exactly, and require the journal date to be no later than the feed's statement cutoff. A difference requires review or the normal accounting correction path. An invoice reference on its own is supplemental evidence.

In one scoped write, reserve the feed line and bank-account journal line, verify that both versions are current and that the journal has no reversal, then attach a receipt and update status. Preserve source account and currency, exact signed amount, selected journal and row identity, reviewer and matching time. An exact replay returns the original receipt. Simultaneous attempts to consume either identity leave one match. Keep historical links if a journal is reversed later, but expose that match as needing review. Reconciliation writes no journal lines or cash balances. Use the transactional boundary proposed in [package 22](opening-balance-receipts.md).

## Route and migration work

Have the feed route and the integration adapter in `rust/apps/api-server/src/records.rs` call the same command. Map their old response aliases from the receipt during the ADR-004 transition. A manual status update can still categorize or queue a line, but a verified reconciliation requires the receipt. Update `apps/flutter/lib/features/banking/data/banking_api_service.dart` to submit the selected posting and preserve the operation key on retry. Keep legacy statuses and links marked unverified until the original journal evidence is reviewed. Use neither a missing reference nor a blank link to infer a match.

Show “Choose a posted entry for this bank account” for inaccessible or unrelated postings, “The bank and journal amounts differ” for sign or amount mismatch, and “This line is already matched. Open the original match” for a competing match.

## Acceptance evidence

- Add `feed_match_rejects_wrong_account_currency_and_direction` to `rust/apps/api-server/tests/bank_feed_http.rs`.
- Add `feed_and_integration_cannot_match_the_same_journal_line_twice` to the provider target proposed in [package 47](subsidiary-journal-posting-receipts.md), with a failure before the last write and a retry after restart.
- Add `legacy_reconciled_without_link_stays_unverified` in `bank_feed_prefix_http.rs` and compare the ledger before and after.

Done when status and receipt survive together and the two route families read the same match.
