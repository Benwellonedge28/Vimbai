# Work package 21 — Preserve an intentional month reopening

Source baseline [d170dff](https://github.com/Benwellonedge28/Vimbai/commit/d170dffe91e2ae8e2e3515f11db386324bb8c6f2). Proposed implementation. [ADR-013](../decisions/ADR-013-autonomy.md) establishes default automatic close and reasoned reopening. The existing [Book policy gate design](book-policy-gate.md) governs who can perform an explicit close or reopen.

## Failure to prevent

`closed_months` in `rust/crates/domains/accounting/src/periods.rs` excludes rows with a reopening date. `close_period` checks that active set and then creates a new close row. The automation handler in `rust/crates/application/automation-hands/src/lib.rs` calls the same function for the requested month. If a second automatic invocation targets a month after a human reopens it, the active-set check allows another close. The existing HTTP test demonstrates reopening and late posting, but does not exercise a later invocation for the same month.

## Transition contract

Use the Book and canonical YYYY-MM as the period's scope. Evaluate all relevant close cycles for that pair, including reopened rows. A reopening belongs to the close record it unlocked. An explicit reclose creates a new close record. Keep the old record together with its reopening reason. A close that was active when a previous request completed must have no effect on the later reopened state merely because the request was retried.

| Current state | Automatic close | Explicit close | Explicit reopen |
| --- | --- | --- | --- |
| No close history | Apply existing close eligibility | Apply existing close eligibility | Refuse with “This month has no active close to reopen.” |
| One active close | Successful no-op with the original close date | Retain the existing already-closed refusal | Update the active row with the date, authority and reason |
| Prior close history, all cycles reopened | Successful no-op with the latest reopening reference | Create a new close cycle after checking the latest reopening date | Refuse with “This month is already open.” |
| More than one active close | Stop and report the inconsistent history | Same | Same |

Reclose date must be on or after the latest recorded reopening date. If it precedes that date, tell the caller “Choose a close date on or after the date this month was reopened.” Find the latest reopening using the canonical reopening date, with stable record ID as the tie-breaker for the reference returned to automation. Ambiguous active history requires repair by an authorised operator before another transition. Report “This month has conflicting close records. Ask a Book administrator to review its close history.” Avoid returning history from another Book.

## File and caller changes

| Existing path and symbol | Required change |
| --- | --- |
| `rust/crates/domains/accounting/src/periods.rs`, `close_period`, `reopen_period`, `closed_months`, `list_periods` | Put the transition matrix in this module. Keep `close_period` as the explicit entry point and add a Rust-only automatic-close entry point returning typed outcomes for closed, already closed and held open. Use complete history when deciding a transition. Treat a store read failure as a failed transition. No client payload field may choose the automatic path. |
| `rust/crates/application/automation-hands/src/lib.rs`, `EngineActions::close_period` | Call the automatic entry point. Return a successful no-op with `closed: false`, `outcome: held_open` and the reopening reference for an intentionally reopened month. Preserve the success response for an already closed month and take the close date from its stored record. Replace matching on the text “already closed” with the typed outcome. |
| `rust/apps/api-server/src/accounting_book.rs`, `close_period` and `reopen_period` | Keep the existing explicit HTTP routes and scope extraction. Apply the Book-policy contract when that gate is available. Expose domain refusal text through the existing error envelope. Cover the worker outcome at its own entry point and keep the explicit API payload compatible. |
| `rust/crates/domains/accounting/src/year_end_close.rs` | Preserve reporting that an earlier year close was superseded by a later reopening, even after an explicit reclose. Include this case in the year-end test coverage. |

Serialise history read and close/reopen mutation for the same Book and period using a store-backed conditional write or transaction. A stale automatic write that loses to reopening must reread the new state and return the held-open outcome. A process-local mutex cannot provide this property across hosted replicas. Keep the check in the store-backed accounting operation shared by hosted and local callers. If any provider cannot offer the required write guarantee, complete its shared store conformance work before enabling the feature on that provider.

The existing posting refusal continues to derive active locks from close records. A late entry after a valid reopening goes through the normal ledger posting path. Reclosing changes the lock only. It cannot create, alter or reverse ledger entries or modify other periods. The public list retains historical and active records and its existing field shapes, so the Flutter reader in `apps/flutter/lib/features/close/data/month_close.dart` requires no protocol update.

## Existing Books and rollout

Existing rows already carry `reopened_on` and `reopen_reason`. Read them in place with the existing IDs. No rewrite of normal close history is required. Before enabling the transition on a store, scan for multiple active close rows per Book and period and report affected record IDs under the operator's scope. Preserve all rows for diagnosis. A future repair requires an explicit accounting decision for each affected period.

## Verification and acceptance

| Test to add | Evidence |
| --- | --- |
| `automatic_close_after_reopening_leaves_the_month_open` in the period tests | Initial automatic close, explicit reopen and a later automatic call leave no active close and one preserved reopening record. |
| `explicit_reclose_preserves_the_prior_reopening` in the period tests | The old record retains its reason, the new cycle is active, and posting is refused for the target month only. |
| `automatic_retry_cannot_overwrite_concurrent_reopening` in shared store conformance | A stale automated decision made against the active row cannot re-lock the month after reopening. Run with two independent handles on both SQLite and PostgreSQL. |
| `reopen_then_run_clock_work_allows_the_late_entry` in `rust/apps/api-server/tests/automation_engine_http.rs` | The route, automation action and late posting agree in the same Book. A second Book's lock is unaffected. |
| `reclosing_a_reopened_month_does_not_restore_year_close_evidence` in the year-end tests | The historical year close continues to show that a relevant month had been reopened. |

The package is complete when the five tests run, the provider race is exercised on both stores without skips, the API contract stays compatible, and the recorded automation outcome explains why the month remains open.
