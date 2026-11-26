# Work package 95 — Keep savings-group contributions and cycle totals in sync

Savings-group baseline [5c2d0018](https://github.com/Benwellonedge28/Vimbai/commit/5c2d0018ed10d212ab43eda94a0a8d2509c3ea0d).

## The two writes

`contribute` in `rust/crates/domains/partnerships/src/groups.rs` creates a contribution and then updates the group's pool. It reads the current cycle before either write. `advance_cycle` updates the cycle independently. A failure or a concurrent advance can leave the contribution and projection inconsistent. The pool is an accounting record of observed group activity.

## Record against a specific observed cycle

A verified contribution requires an active scoped member, group ID, expected cycle and group revision, exact positive amount in the group's currency, received date and external proof reference. Accept a contribution only in the group's observed current cycle for this initial contract. A late observation for a past cycle needs an explicitly authorized correction on that cycle. The group does not hold or move the member's money.

In one group-scoped transaction reserve the receipt key, verify member, cycle and revision, append the contribution and update exact cycle and group projections. Advance cycle uses the same revision guard and preserves the old cycle totals. A matching key replays the first receipt before reading the new current cycle. A conflicting key or a cycle change returns a conflict with the relevant current cycle. Allow corrections through a new dated receipt naming the affected contribution and cycle. Any journal effect is handled through the accounting path independently.

## Callers and prior contributions

Update `contribute`, `list_contributions`, `advance_cycle` and their adapters in `rust/apps/api-server/src/records.rs`. Keep the existing response keys and return verified receipt identity for opted-in groups. Filter Book, group and cycle before paging. Reconcile older contributions to cached balances from the complete group history. A discrepancy stays visible until a reviewer confirms an opening checkpoint or a correction. Existing receipt IDs are never replaced.

Show “This group moved to another cycle. Review which cycle received the contribution” for a stale cycle and “Choose an active member of this group” for an unavailable member.

## Verification

- Add `group_contribution_replay_does_not_move_to_new_cycle` to `rust/apps/api-server/tests/records_http.rs`.
- Add `group_pool_and_contribution_are_atomic_during_cycle_advance` to the provider target in [package 47](subsidiary-journal-posting-receipts.md), including an injected update failure.
- Add `legacy_group_balance_drift_stays_visible_until_review` beside the group tests.

Accept when the original cycle and amount can be verified after an advance, and the total agrees with the retained contribution receipts.
