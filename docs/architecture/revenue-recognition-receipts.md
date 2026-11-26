# Work package 45 — Commit recognition progress and its journal together

Proposed design against `b8826fac84d19d1792f7f95dc552f32d8f053755`.

## Observed behavior
`rust/crates/domains/accounting/src/revenue_recognition.rs::recognise_revenue` derives cumulative progress and posts the newly earned amount before updating the contract's obligations. The last journal ID field does not identify every historical recognition.

## Command contract
Add an operation ID scoped to Book, contract and obligation. Bind it to input amount, resolved effective date, and supplied account selectors. Under a contract transaction guard, re-read obligation progress and apply the existing `recognise` calculation once. Save the prior and resulting progress, earned increment and journal ID in an operation receipt. For positive increments commit the journal, progress and receipt together. Preserve the core posting path, dimensional scope, and Money on both sides. A zero increment returns its own receipt without a ledger entry. The total recognised must never exceed the allocated obligation price. A conflicting replay returns “This recognition was already submitted with different details.” A new positive increment in a closed period retains the existing refusal.

Keep the existing adapter in `rust/apps/api-server/src/records.rs::recognise_revenue` and current response values. An authorised matching replay returns original figures even after other obligations change. Use the contract transaction guard for the whole obligations array so concurrent progress on different obligations cannot overwrite unrelated progress.

## Existing contracts and acceptance
Keep legacy obligation progress and journal IDs. Flag an unmatched recognition journal for review against its Book, contract and obligation reference. Repair the projection only from verified evidence.

- `recognition_failure_rolls_back_entry_and_progress` faults the receipt write on SQLite and PostgreSQL.
- `concurrent_recognition_keeps_both_obligations_and_contract_total` covers different obligations in one contract.
- `recognition_same_operation_replays_and_conflicting_amount_refuses` checks exact persistence and the route response.
- `recognition_cannot_exceed_the_allocated_price` verifies that racing requests do not overstate revenue.

Done when recognised totals agree with receipt and ledger increments after a dropped reply and restart.
