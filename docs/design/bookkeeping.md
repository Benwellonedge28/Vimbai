# Bookkeeping workspaces
Status: proposed extension of ../domains/accounting.md and ../domains/ledger.md.

## Structure
Bookkeeping → capture/inbox, journals, accounts and ledgers, reconciliation, periods, adjustments and closing. Sales/purchases, banking, assets, payroll where supported and other domains link to their own workspaces. Reporting is separate and read-only with respect to posted records.

## Workflow
Capture fact → validate document and policy → resolve account → draft → approval if required → kernel posting → reconciliation → report. Keep draft, approved, posted, reversed and rejected states distinct. ADR-013 autonomy avoids asking users for account numbers; missing mandatory policies are explicit errors. ADR-015 provisional account creation is proposed work; control accounts never auto-created.

## Ownership
Accounting domain orchestrates journal and period rules; ledger-core owns posting; ledger-services reconciles. Extend existing accounting feature and books inbox; do not implement another journal store. Cross-domain document references remain stable and traceable.

## Acceptance
Duplicate submissions post once; closed periods reject; unauthorized approval fails; reversal preserves original; provisional creation is atomic; control-account reconciliation agrees with ledger; workspace navigation works offline with explicit pending status.
