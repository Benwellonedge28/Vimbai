# Work package 93 — Record external settlement evidence for a partner change

Partner-change baseline [5c2d0018](https://github.com/Benwellonedge28/Vimbai/commit/5c2d0018ed10d212ab43eda94a0a8d2509c3ea0d).

## What the state records today

`settle_change` in `rust/crates/domains/partnerships/src/changes.rs` loads a scoped record and marks it settled. Repeating it is allowed and the action has no receipt input. `retire` and `record_death` create change records with calculated amounts; those amounts do not prove money changed hands.

## Attach a receipt to the amount due

For a versioned departure require a frozen partner-change ID and applicable agreement revision, due amount, currency and effective date. A settlement recording names an external reference, effective date, exact amount and method of observation. Support partial and full settlement of a verified nonnegative amount due. For a zero due amount an authorized closure requires a reason rather than a zero transfer. Historical settlement evidence can link a visible posted journal and must agree with the partner, currency and amount. The command records completed events and does not send payment instructions.

Scope idempotency by Book, change ID and reference. Under one revision guard, sum verified outstanding receipt movements, refuse excess, insert the receipt, update settled-to-date and set the current state to pending, partial or settled. Replay returns the original receipt and actor. An authorized correction creates a dated inverse receipt and can reopen the outstanding amount. Accounting separately reverses posted journal evidence where needed.

## Publish and migrate

Keep the existing `settle_change` adapter in `rust/apps/api-server/src/records.rs` and add the versioned input contract and the verified receipt list. Block unguarded changes on verified departures. Read old settlement flags as legacy assertions. A reviewer must supply external evidence before deriving an amount settled. An old status alone cannot establish a date or receipt.

Show “Provide evidence of the amount already settled” for missing proof, “This amount exceeds what remains due” for excess and “This reference already identifies another receipt” for changed replay.

## Proof

- In the changes module add `partial_partner_settlement_preserves_exact_outstanding_and_date`.
- Add `legacy_partner_settled_flag_does_not_create_verified_receipt` to `rust/apps/api-server/tests/records_http.rs).
- Add `partner_receipt_and_status_commit_together_on_both_stores` to [package 47](subsidiary-journal-posting-receipts.md), with competing settlements and replay after restart.

Accept when a settled departure can be explained by distinct evidenced receipts or an explicit zero-due closure.
