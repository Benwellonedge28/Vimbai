# Work package 53 — Reconcile closing capital with every recorded movement

Source baseline [ca0a15a4](https://github.com/Benwellonedge28/Vimbai/commit/ca0a15a467b3263bf2ae5a5342ee30ca2120252e).

## The zero-result branch

`create_capital_calculation` in `rust/crates/domains/accounting/src/bookkeeping.rs` totals contributions and withdrawals from `entries`. When neither headline earnings amount is positive, it returns opening capital as closing capital even if those movements changed the owner's capital. The same function keeps profit and loss rows but never includes their amounts. If both headline values are positive, the profit branch wins. The test `a_capital_calculation_given_no_profit_reports_the_opening_capital_back` documents the legacy result.

## Make the earnings source explicit

Accept the additive `calculation_version: "2"` option shared with the [statement of affairs](statement-of-affairs-revisions.md). Introduce a typed input and one pure calculation in `bookkeeping.rs`. The request supplies an inclusive period and `as_of_date` equal to the period end. Use the [working-paper amount validator](statement-of-affairs-revisions.md#save-a-dated-working-paper) for all decimal Book-currency inputs. Opening capital may be signed. Each contribution and withdrawal has a positive amount, a date inside the period, and a stable row ID unique to the paper. Accept only `additional_capital`, `withdrawal`, `net_profit` and `net_loss` as movement types.

The caller selects the earnings source using `earnings_basis`. When the source is `summary`, accept headline `net_profit` and `net_loss`, each nonnegative and at most one positive. Reject earnings lines. When the source is `entries`, require any earnings amounts to appear as dated lines and reject headline fields. Sum profit and loss lines separately and net the sums to produce mutually exclusive headline outputs. An explicit zero-profit period can be represented by two zero summary values or by the absence of earnings lines in the entries mode. Compute closing capital as opening plus contributions less withdrawals plus net earnings. Persist the input source, exact amounts and final totals in `CAPITAL_CALCULATION`, along with the authenticated preparer and version. Do the computation before storing a single record. A negative result remains visible.

Example with zero earnings. Opening 700, contributions 110 and withdrawals 40 produce closing capital 770. In entries mode, profit 25 and loss 40 give net loss 15, so the same movements produce closing capital 755.

## Preserve the existing register

`create_capital`, `list_capital` and `get_capital` in `rust/apps/api-server/src/accounting_book.rs` serve both accounting prefixes. Carry the new version through create, list and lookup and retain existing response keys. Unversioned callers keep the old formula while their input contract is supported. This calculation is a saved working paper under ADR-013. It is outside the ledger posting path and can be prepared for an open or a closed period.

Keep legacy figures and IDs. On request, recompute a historical paper only as a new versioned record carrying its predecessor ID. Require the preparer to choose the earnings source if both a header and rows carry earnings. Keep old reports identifiable as historical calculations even where the figures differ. There is no migration that rewrites stored closing capital.

Show “Choose whether the result comes from your period total or the dated entries” for mixed sources, “Enter either a net profit or a net loss for the period” for two positive headline values, and “This movement falls outside the selected period” with its row ID for an out-of-range date. For an unreadable or unsupported movement amount, identify the row and keep the form available for correction.

## Verification before adoption

- In the bookkeeping unit tests, add `capital_with_no_earnings_still_applies_contributions_and_withdrawals` and `capital_entry_earnings_are_netted_once`. Exercise zero, positive and negative capital outcomes and an exact amount with many decimal digits.
- Add `capital_conflicting_earnings_sources_save_no_paper` to `rust/apps/api-server/tests/records_http.rs`. Include mixed sources and a movement outside the period and verify the scoped list is unchanged.
- Add `capital_version_preserves_legacy_value_and_exact_new_readback` to the same HTTP suite. Compare a legacy result and the new calculation for the same facts, and check that the records alias returns the stored result by ID.

The package is complete when the saved versioned closing balance can be independently reconstructed from its recorded basis and movements without accessing any ledger entry.
