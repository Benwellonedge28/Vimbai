# Work package 44 — Verify an interdepartmental bill before approval

Proposed design against `b8826fac84d19d1792f7f95dc552f32d8f053755`.

## Observed behavior
`rust/crates/domains/accounting/src/departments.rs::approve_inter_department_bill` updates bill status before resolving departments and constructing a journal proposal. It falls back to synthetic department account codes when mappings are absent.

## Approval transition
Resolve the bill plus both department IDs inside the same Book, validate distinct departments, and read configured cost and revenue accounts. For missing mapping say “Set the cost or revenue account on the named department before approving.” Refuse an invalid amount, missing department, invalid billing date or closed posting period before writing anything. Validate active accounts and their distinct purposes. Resolve the actor from authenticated approval context. An additive `approval_id` binds a replay to the original result. Commit the approved bill state and the exact proposed debit/credit lines and resolved account IDs together. Neither the proposal nor approval is a ledger posting.

In `rust/apps/api-server/src/records.rs::approve_inter_department_bill`, preserve the result shape and label the lines as a proposal. Even after account mappings change, replay returns the approved proposal from the saved evidence. A later authorised posting must recheck the period and use ledger-core. A stale or changed bill returns “The bill changed before approval. Review it again.”

## Historical evidence and tests
Retain approvals without evidence and display them as legacy. Never manufacture a posting link from a bill number.

- `bill_approval_with_missing_department_changes_no_status` checks scope and incomplete setup.
- `bill_approval_uses_configured_distinct_accounts` verifies exact balanced proposal lines and zero posted journals.
- `bill_approval_and_evidence_commit_together` injects a write fault on both stores.
- `bill_approval_replay_keeps_original_actor_and_lines` changes mappings before the replay.

Done when a bill reaches approved only with retrievable proposal evidence and both route aliases return the same scoped result.
