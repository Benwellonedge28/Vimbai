# Work package 75 — Keep treasury limit decisions tied to an immutable policy revision

Policy module baseline [3a2c2ca1](https://github.com/Benwellonedge28/Vimbai/commit/3a2c2ca16978df88a95ad6b1c1decb2148a8707b).

## Why a utilization value is insufficient

`set_limit` in `rust/crates/domains/treasury/src/treasury_policy.rs` saves the limit and a default warning threshold. `check_compliance` inserts a check, then separately stores the checked amount as the limit's current utilization. A later check with an older effective date can replace that display value. Neither operation records a frozen observation source and policy revision together.

## Capture the observed decision

A versioned limit stores the Book policy ID, limit type, currency, nonnegative exact limit, effective start, revision and reviewer. Changing a limit appends a new revision under the same stable identity. Each check provides a stable observation key, effective date, measured amount, matching currency and source reference. Resolve the revision applicable on that date under a consistent read. A measured amount equal to the limit passes. A zero limit compares normally and reports utilization percentage as null. Do not emit a threshold alert or apply this legal or operational decision outside the Book.

Within one scoped transaction store the check with policy and limit revisions, input and provenance, outcome, and authenticated actor. Update the current observation pointer only when its effective date is newer, using saved record sequence for same-date ties. A matching key replays the saved decision. A key with altered evidence fails. A failed pointer write rolls back the check. Historical verdicts remain attached to the policy they evaluated and changes never rewrite posted accounting entries.

## Routes and existing policy records

Extend `set_policy_limit`, `check_policy_limit`, `policy_limits` and `policy_compliance_checks` in `rust/apps/api-server/src/records.rs` under the treasury policy route family. Carry both current observed amount and percentage with distinct fields in the opt-in response. Preserve the legacy utilization amount and leave legacy warning metadata readable while migrating. Filter by Book and policy before paging checks. An earlier saved check retains its amount and outcome, marked unverified for policy revision unless an exact match can be demonstrated. No backfill substitutes the latest limit.

Show “Choose a limit that applied on the observation date” when absent, “Enter the observation currency used by this limit” when currencies differ, and “A different check already uses this reference” for a changed replay.

## Acceptance

- In the domain tests add `limit_equality_passes_and_zero_limit_has_no_percentage`.
- In `rust/apps/api-server/tests/records_http.rs` add `older_check_does_not_replace_latest_utilization` and `historic_check_keeps_its_limit_after_policy_change`.
- In the provider target from [package 47](subsidiary-journal-posting-receipts.md), add `limit_check_and_current_pointer_roll_back_together`, covering concurrent observations and replay.

Accept when an auditor can recalculate an earlier result without reading the current policy.
