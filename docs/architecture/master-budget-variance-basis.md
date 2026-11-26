# Work package 63 — Report budget variance only where actuals are evidenced

Reviewed against [ca0a15a4](https://github.com/Benwellonedge28/Vimbai/commit/ca0a15a467b3263bf2ae5a5342ee30ca2120252e).

## Current interpretation

`detailed_variance` in `rust/crates/domains/budgeting/src/master_budget.rs` defaults omitted amounts to zero, treats every negative variance as favorable, and returns fixed causes and corrective actions. The Flutter method `getBudgetVarianceAnalysis` in `apps/flutter/lib/features/budgeting/data/finance_api_service.dart` also sends a zero for an absent actual. `budget_variance` in `rust/apps/api-server/src/records.rs` delegates to the calculator. The [spending-plan comparison](book-budget-revision-evidence.md) uses scoped ledger data. This package covers the separate direct-input comparison.

## Explicit observation and calculation

Offer a version 2 request. Each row carries a stable line ID, display label, role `revenue` or `expense`, exact budget, and an actual represented by either a decimal string or null. A decimal actual requires a source reference and observation date within the submitted period. An omitted actual is treated as unknown. Each actual and budget uses the declared currency. Within a row, variance is actual less budget. Positive variance is favorable for revenue and unfavorable for expense. Use absolute magnitudes for favorable and unfavorable totals and expose observed revenue and expense results separately. A null actual has null variance and no contribution to totals. Report the number and IDs of incomplete rows so subtotal coverage is explicit.

Compute percent from variance over absolute budget and return null for a zero denominator. A caller can supply a nonnegative analysis tolerance for this run. A row is outside tolerance when absolute percent is strictly greater than it. No threshold or default is saved to the Book and no notification is emitted. For a zero budget with nonzero actual, identify the line separately as unbudgeted activity. Do not manufacture causal evidence. Version 2 returns an empty causes list unless the caller supplies attributed observations.

For a revenue plan of 100 with actual 80, report a 20 adverse variance. For an expense plan of 100 with actual 80, report a 20 favorable variance. A third line with actual null stays incomplete in both summaries.

## Contract and migration

Keep the legacy request and response intact in `budget_variance` while the opt-in contract is adopted. Update the Flutter adapter to take an observation with amount, effective date and source reference for a known actual, then send actual null when it has no observation. Parse exact strings or null, and update `finance_models.dart` and `budget_variance_report_page.dart` in the budgeting feature to distinguish unknown and unbudgeted. Use the opt-in route behavior in both engine and gateway contract tests. This analysis saves no result in the engine and requires no server migration. Existing local plans remain viable, but default-filled zero actuals with no independent evidence must come through as unknown.

Show “Enter the source and date for this actual” when a value has no provenance, “Identify this line as revenue or expense” for an unsupported role and “Enter a nonnegative tolerance or leave it blank” for an invalid tolerance.

## Acceptance evidence

- Add `budget_variance_distinguishes_revenue_expense_and_unknown_actuals` to the module tests. Use equal variances of both roles and a real zero actual.
- Add `budget_zero_denominator_has_no_percentage_and_is_reported_separately` beside it.
- Add `budget_variance_v2_http_has_no_default_causes` in `rust/apps/api-server/tests/records_http.rs`. Preserve the legacy contract in the existing fixture.
- Add `unknown_actual_survives_budget_comparison_transport` to the Flutter budgeting tests.

Complete when the response reconciles from the observed lines and clearly identifies any figure absent from the comparison.
