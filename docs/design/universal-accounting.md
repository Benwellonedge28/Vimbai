# Universal accounting
Status: proposed extension of existing domain specifications and ADR-002/014.

## Model
One Book, one functional currency, configurable chart, fiscal calendar, dimensions, recognition policies and standards profile. Personal, household, sole trader, partnership, company, nonprofit and government profiles are data and capabilities, not separate engines. Journal entries represent economic facts; reports derive from posted records.

## Ownership
Accounting owns period, journal orchestration and reporting policy. Equity owns ownership/capital events; NPO owns restricted funds; banking owns account reconciliation; treasury owns liquidity planning; receivables/payables own obligations; assets own lifecycle; costing owns allocations; taxation owns jurisdiction rules. Review overlapping company_accounting, fund_accounting, cashbook and control-account modules before moving anything. Cross-domain composition stays above domains. No duplicate balances or private ledgers.

## Contracts
Domain posting intent includes source ID, Book, date, policy version, dimensions, exact amounts and idempotency key. Kernel validates balanced lines and immutable chain. Reports use consistent as-of snapshots and reconcile to trial balance.

## Acceptance
Golden cases for each profile, cross-domain reconciliation, period close, corrections, consolidation and exact arithmetic. Policy changes versioned and effective-dated; historic results reproducible. File-level ownership changes require source audit and compatibility mapping.
