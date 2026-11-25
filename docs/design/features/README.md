# Feature design register

Source: catalog/services/catalog.yaml at 2064d885c4082ed6632c567e19e305cbb3bfa21f. All 143 catalogue services are inventoried below. A route is evidence of a published contract, not verified implementation correctness. Additional unlisted routes, UI features and project proposals require separate inventory. Accepted ADRs override stale catalogue descriptions. Each service specification must be reviewed against its actual handlers before implementation.

## accounting

- accounting-service — `rust/crates/domains/accounting`
- accounting-standards-service — `rust/crates/domains/accounting`
- balance-sheet-service — `rust/crates/domains/accounting`
- cash-flow-service — `rust/crates/domains/accounting`
- cash-flow-statement-service — `rust/crates/domains/accounting`
- cashbook-service — `rust/crates/domains/accounting`
- company-accounting-service — `rust/crates/domains/accounting`
- departmental-accounting-service — `rust/crates/domains/accounting`
- fund-accounting-service — `rust/crates/domains/accounting`
- ifrs-reporting-service — `rust/crates/domains/accounting`
- intercompany-service — `rust/crates/domains/accounting`
- revenue-recognition-service — `rust/crates/domains/accounting`
- trading-account-service — `rust/crates/domains/accounting`

## admin

- admin-service — `rust/crates/platform/admin`

## alerts

- alerts-service — `rust/crates/platform/alerts`

## analytics

- performance-benchmarking-service — `rust/crates/domains/analytics`

## assets

- biological-assets-service — `rust/crates/domains/assets`
- disposal-account-service — `rust/crates/domains/assets`
- disposal-group-service — `rust/crates/domains/assets`
- fixed-assets-register-service — `rust/crates/domains/assets`
- fixed-assets-schedule-service — `rust/crates/domains/assets`
- intangible-assets-service — `rust/crates/domains/assets`
- investment-property-service — `rust/crates/domains/assets`
- lease-termination-service — `rust/crates/domains/assets`
- revaluation-model-service — `rust/crates/domains/assets`
- revaluation-reserve-service — `rust/crates/domains/assets`

## audit

- audit-compliance-service — `rust/crates/domains/audit`
- audit-engagement-service — `rust/crates/domains/audit`
- audit-sampling-service — `rust/crates/domains/audit`

## authentication

- mfa-auth-service — `rust/crates/platform/identity`

## automation

- automation-engine-service — `rust/crates/domains/automation`

## banking

- bank-feed-service — `rust/crates/domains/banking`
- bank-reconciliation-service — `rust/crates/domains/banking`
- bank-relationship-service — `rust/crates/domains/banking`
- banking-integration-service — `rust/crates/domains/banking`
- currency-service — `rust/crates/domains/banking`

## budgeting

- activity-based-budget-service — `rust/crates/domains/budgeting`
- appropriation-control-service — `rust/crates/domains/budgeting`
- budgeting-service — `rust/crates/domains/budgeting`
- rolling-forecast-service — `rust/crates/domains/budgeting`
- scenario-analysis-service — `rust/crates/domains/budgeting`
- scenario-modeling-service — `rust/crates/domains/budgeting`
- sensitivity-analysis-service — `rust/crates/domains/budgeting`
- zero-based-budgeting-service — `rust/crates/domains/budgeting`

## cache

- cache-service — `rust/crates/platform/cache`

## calculations

- calculation-api — `rust/crates/application`
- calculator-catalogue — `rust/crates/application`
- realtime-calculation-engine — `rust/crates/application`

## compliance

- privacy-admin-dashboard-service — `rust/crates/domains/compliance`
- regulatory-compliance-service — `rust/crates/domains/compliance`
- risk-assessment-service — `rust/crates/domains/compliance`
- sox-compliance-service — `rust/crates/domains/compliance`

## costing

- absorption-costing-service — `rust/crates/domains/costing`
- absorption-costing-statement-service — `rust/crates/domains/costing`
- basis-apportionment-service — `rust/crates/domains/costing`
- cvp-analysis-service — `rust/crates/domains/costing`
- job-costing-service — `rust/crates/domains/costing`
- overhead-absorption-rate-service — `rust/crates/domains/costing`
- process-costing-service — `rust/crates/domains/costing`
- product-costing-service — `rust/crates/domains/costing`
- selling-price-reduction-service — `rust/crates/domains/costing`

## data

- data-warehouse-service — `rust/crates/domains/data`
- encrypted-backup-service — `rust/crates/domains/data`
- etl-service — `rust/crates/domains/data`

## documents

- document-service — `rust/crates/platform/documents`

## equity

- authorized-share-capital-service — `rust/crates/domains/equity`
- bonus-shares-service — `rust/crates/domains/equity`
- capital-reconstruction-service — `rust/crates/domains/equity`
- capital-redemption-reserve-service — `rust/crates/domains/equity`
- debentures-service — `rust/crates/domains/equity`
- equity-changes-service — `rust/crates/domains/equity`
- general-reserve-service — `rust/crates/domains/equity`
- initial-investment-service — `rust/crates/domains/equity`
- issued-share-capital-service — `rust/crates/domains/equity`
- ordinary-shares-service — `rust/crates/domains/equity`
- preference-shares-service — `rust/crates/domains/equity`
- retained-profits-service — `rust/crates/domains/equity`
- right-issues-service — `rust/crates/domains/equity`
- share-premium-service — `rust/crates/domains/equity`
- share-redemption-service — `rust/crates/domains/equity`

## events

- message-bus-service — `rust/crates/platform/events`
- websocket-service — `rust/crates/platform/events`

## finance

- continue-shutdown-decision-service — `rust/crates/domains/finance`
- insurance-claims-service — `rust/crates/domains/finance`
- internal-rate-return-service — `rust/crates/domains/finance`
- investment-monitoring-service — `rust/crates/domains/finance`
- net-present-value-service — `rust/crates/domains/finance`
- payback-period-service — `rust/crates/domains/finance`
- personal-finance-service — `rust/crates/domains/finance`
- trade-finance-service — `rust/crates/domains/finance`

## identity

- financial-identity-service — `rust/crates/domains/identity`

## integrations

- graphql-service — `rust/crates/platform/integrations`

## invoicing

- invoice-customers-service — `rust/crates/domains/invoicing`
- invoices-service — `rust/crates/domains/invoicing`
- invoicing-service — `rust/crates/domains/invoicing`
- subscription-plans-service — `rust/crates/domains/invoicing`

## kernel

- control-account-reconciliation-service — `rust/crates/kernel/ledger-services`
- double-entry-principles-service — `rust/crates/kernel/ledger-services`
- financial-integrity-service — `rust/crates/kernel/ledger-services`
- financial-state-machine-service — `rust/crates/kernel/ledger-services`

## multimodal

- multimodal-pipeline-service — `rust/crates/domains/multimodal`
- tasks-service — `rust/crates/domains/multimodal`

## notifications

- notifications-service — `rust/crates/platform/notifications`

## npo

- npo-service — `rust/crates/domains/npo`
- npo-scale-service — `rust/crates/domains/npo`

## partnerships

- family-community-group-service — `rust/crates/domains/partnerships`
- partnership-agreement-service — `rust/crates/domains/partnerships`
- partnership-changes-service — `rust/crates/domains/partnerships`
- partnership-dissolution-service — `rust/crates/domains/partnerships`
- partnership-revaluation-service — `rust/crates/domains/partnerships`

## payables

- expense-tracking-service — `rust/crates/domains/payables`
- purchases-ledger-control-service — `rust/crates/domains/payables`

## payments

- pos-integration-service — `rust/crates/domains/payments`

## payroll

- benefits-admin-service — `rust/crates/domains/payroll`

## receivables

- bad-debts-recovery-service — `rust/crates/domains/receivables`
- provision-bad-debts-service — `rust/crates/domains/receivables`
- provision-doubtful-debts-service — `rust/crates/domains/receivables`
- sales-ledger-control-service — `rust/crates/domains/receivables`

## reporting

- dashboard-service — `rust/crates/domains/reporting`
- report-automation-service — `rust/crates/domains/reporting`
- report-distribution-service — `rust/crates/domains/reporting`
- reporting-service — `rust/crates/domains/reporting`
- reports-service — `rust/crates/domains/reporting`

## security

- zero-trust-data-service — `rust/crates/domains/security`

## sync

- book-sync-service — `rust/crates/platform/offline-sync`

## taxation

- tax-accounting-service — `rust/crates/domains/taxation`
- tax-compliance-service — `rust/crates/domains/taxation`
- tax-planning-service — `rust/crates/domains/taxation`

## treasury

- cash-management-service — `rust/crates/domains/treasury`
- cash-optimization-service — `rust/crates/domains/treasury`
- debt-management-service — `rust/crates/domains/treasury`
- foreign-exchange-service — `rust/crates/domains/treasury`
- petty-cash-service — `rust/crates/domains/treasury`
- sovereign-treasury-service — `rust/crates/domains/treasury`
- treasury-analytics-service — `rust/crates/domains/treasury`
- treasury-compliance-service — `rust/crates/domains/treasury`
- treasury-management-service — `rust/crates/domains/treasury`
- treasury-policy-service — `rust/crates/domains/treasury`
- treasury-reporting-service — `rust/crates/domains/treasury`
- treasury-risk-service — `rust/crates/domains/treasury`

## webhooks

- webhook-service — `rust/crates/platform/webhooks`

## workflow

- distributed-workflow-orchestration-service — `rust/crates/domains/workflow`
- workflow-service — `rust/crates/domains/workflow`

## Additional project proposals

Book navigation, granular access, commercial subscriptions (distinct from customer subscription-plans-service), referrals, partners, self-hosting, government, AI, data governance and distribution: see parent design collection. Vimbai Pay excluded by ADR-005; developer platform deferred by ADR-006.
