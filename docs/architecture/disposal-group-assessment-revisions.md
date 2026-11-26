# Work package 60 — Tie each disposal-group assessment to a frozen member list

Proposed design based on [ca0a15a4](https://github.com/Benwellonedge28/Vimbai/commit/ca0a15a467b3263bf2ae5a5342ee30ca2120252e).

## Evidence can drift between writes

In `rust/crates/domains/assets/src/disposal_groups.rs`, `add_asset` writes the member before updating group totals. The group member reader filters a capped collection. `impairment_test` calculates against the cached carrying total, writes an assessment and then updates the header separately. `update_status` accepts the requested status without checking a prior assessment revision.

## Version the group composition

Use Book scope plus group ID and registered asset ID as the membership key. A verified membership carries a stable in-Book asset reference, effective date, valuation source, carrying amount and currency. Lock the group version, scope before reading members and apply the member change and aggregate totals in one transaction. A matching retry returns the member result. Keep succession evidence when a member amount changes. No correction edits a posted entry or disposes of the underlying asset. An optimistic version mismatch returns “This group changed while you were reviewing it. Reload the member list.”

An assessment takes the expected group version, valuation date, approved fair value less costs, external evidence reference and authenticated reviewer. Require at least one verified member and a complete revision read. Store sorted member IDs and versions, their exact aggregate carrying amount, the submitted valuation and the exact nonnegative shortfall. Compute that shortfall as the greater of zero and carrying amount less fair value less costs. Atomically save the assessment and its current pointer. A membership change leaves prior assessments readable but marks the current assessment outdated. A new assessment for a revised group cannot silently replace any recorded historical loss. This is a working-paper assessment and does not by itself recognize an impairment posting.

## Commands and transition

Keep `add_disposal_asset`, `disposal_impairment_test`, `disposal_group_status` and `one_disposal_group` in `rust/apps/api-server/src/records.rs`. Expose the new guarded behavior via an explicit evidence-version option on both disposal-group route prefixes. For a verified group, allow held-for-sale to transition to sold or withdrawn with an effective date and supporting reference. Require a current assessment for sold. Make both terminal states read-only, preserving the last evidence. Reject legacy unguarded writes on an opted-in group. An outdated valuation says “The member list has changed. Review a new assessment before marking the group sold.”

Existing groups keep their member and test IDs. Classify unlinked asset references and header discrepancies as review items. Migrate a group by confirming and linking its members against the same Book, then recalculating current totals from the complete reviewed membership. Historical impairment values remain as entered and get a legacy label. A prior assessment without a reconstructable member revision is never marked verified.

## Tests and completion

- Add `disposal_assessment_values_exact_member_revision` in the module tests. Use a one-unit shortfall at stored monetary precision and a member amount changed after the test.
- Add `disposal_group_rejects_sale_with_outdated_assessment` to `rust/apps/api-server/tests/records_http.rs`, with a repeat through the records alias and an inaccessible group.
- Add `disposal_group_totals_survive_member_and_assessment_races` to the shared accounting provider target from [package 47](subsidiary-journal-posting-receipts.md). Check a failure between writes and two concurrent member updates on both providers.

Complete when an independent reviewer can reproduce any new assessment from its saved members and can see exactly when it ceased to apply.
