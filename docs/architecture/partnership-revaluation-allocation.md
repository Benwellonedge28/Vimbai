# Work package 96 — Tie partnership revaluation proposals to dated asset and agreement evidence

Partnership revaluation source [5c2d0018](https://github.com/Benwellonedge28/Vimbai/commit/5c2d0018ed10d212ab43eda94a0a8d2509c3ea0d).

## Current proposal behavior

`revalue` in `rust/crates/domains/partnerships/src/revaluation.rs` uses supplied old and new figures, emits a draft for unchanged valuations, and writes draft lines with fixed reserve, loss and goodwill codes. Saved reports contain draft lines with empty posted journal IDs. These rows describe a proposal.

## Build a reviewable dated calculation

For a verified proposal require the applicable Book agreement revision, valuation date, independently reviewed old carrying-value basis and unique scoped asset IDs. Each asset records Book currency, exact old and proposed new amounts, approved valuation reference and effective date. Equal amounts produce a zero movement with no draft line. Sum increases and decreases separately, with net change equal to increases less decreases. Where an approved agreement directs partner allocation, distribute the signed net result by its frozen fractions using largest-remainder rounding. Keep the allocation and verify it sums exactly to the net movement.

Require an explicit goodwill treatment and supporting approval when goodwill is present. Resolve each proposed chart role from the Book through the [account-resolution design](posting-account-resolution.md). If required chart roles or policy are missing, save only a calculation result with a visible proposal refusal, never an invented code. If a draft is available, verify debit and credit totals exactly and pin every draft line to an asset or goodwill evidence item. This service does not post. The normal accounting command enforces period and authorization if an approved proposal is taken forward.

## Route and migration

Extend the `partnership_revaluation::revalue` adapter and corresponding reads in `rust/apps/api-server/src/records.rs` with the evidence version. Keep old output keys. Use an operation key for the verified run, and retain original evidence when a new run supersedes it. Preserve legacy drafts and do not mark them eligible for replay or infer posted IDs. A repeated run with changed evidence produces a conflict or a new reviewed revision.

Show “Configure the required revaluation accounts before preparing the journal proposal” for missing mappings and “This asset appears more than once” for a repeated ID.

## Acceptance

- In the revaluation module add `unchanged_asset_creates_no_draft_and_allocations_tie_exactly`.
- Add `revaluation_proposal_uses_book_chart_roles_and_reports_missing_mapping` with two differently numbered charts.
- In `rust/apps/api-server/tests/records_http.rs`, add `revaluation_saved_inputs_survive_agreement_revision_without_posting`.

Complete when a saved proposal reconciles to its asset evidence and cannot be confused with a posted revaluation.
