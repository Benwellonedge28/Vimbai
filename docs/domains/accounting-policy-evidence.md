# Version accounting standards and preserve evidence for each report

Source baseline `2df3a7685b1109756fad0cca4d7b648b25790bdb`. Depends on the Book policy and access rules in 01 and 03. Applies ADR-002, ADR-012 and ADR-013.

## Existing behavior

`rust/crates/domains/accounting/src/standards.rs` lists standards, stores account mappings and policies, and supports a compliance check. In `run_compliance_check`, a requirement without supplied evidence starts as pass, and previous rows are deleted before new rows are written. `rust/apps/api-server/src/accounting_standards.rs` publishes the standards surface. `rust/crates/domains/accounting/src/ifrs_reports.rs` holds reports, notes and approvals. `rust/apps/api-server/tests/accounting_standards_http.rs` covers the existing HTTP contract.

## Reporting policy and read path

Represent each approved standard as an immutable policy version with an issuing body, edition, source provenance, effective interval and an explicit requirement inventory. Store versioned Book assignments and mapping records that a Book can override only through an authorized and recorded effective-dated action. An organisation default can propose a policy for its Books. Each report pins the actual version and mappings used by that Book. Detect overlapping assignments and unmapped accounts explicitly. Derive financial figures from the functional-currency ledger and apply presentation translation as a read projection. Use `rust/crates/domains/accounting/src/ledger_book.rs` to read balances and source entry IDs, preserving exact money and traceability.

Make each check run append-only and include run ID, Book, period, policy edition, source report revision and an evidence digest. Report pass, fail, not assessed and not applicable as separate states. A missing evidence record becomes not assessed. Treat a user attestation as evidence attributed to that user and distinguish it from an engine-verified check. Approval records a named reviewer and exact report revision; a mapping or input change creates a new revision. A published export names the framework, version, exclusions, results and whether it was approved. Do not label a catalogue entry as supported coverage until a checked fixture validates the requirements claimed.

## Legacy data and operator messages

Keep existing configuration IDs and map them to an explicitly labelled legacy version at migration. Bind past reports to their recorded figures and observed evidence. Preserve legacy check rows as historical where recoverable. Their missing evidence must not be inferred or selectively upgraded to verified. New reports show “This line needs a mapping under the selected policy” and link to the Book’s mapping screen. A failed write leaves the previous policy configuration usable. Preserve current standards list and read routes through their existing contracts.

## Acceptance tests

- `missing_evidence_is_not_assessed` exercises the same endpoint with an empty validation list and a mixed list.
- `replacing_a_policy_keeps_the_previous_approved_report_reproducible` checks reports before and after the effective date.
- `unmapped_accounts_are_visible_and_block_approval` prevents a balanced but incomplete presentation from being signed off.
- `translated_report_drills_back_to_the_same_functional_postings` verifies the ADR-012 boundary.
- Done when an independent accounting reviewer approves the first named reporting profile against an agreed fixture and every claimed requirement is linked to evidence.
