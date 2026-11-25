# Monetisation architecture
Status: proposed; separate from customer bookkeeping and ADR-005.

## Ownership
A commercial bounded context owns product catalogue, price versions, subscriptions, entitlements, referral awards and partner commissions. Proposed rust/crates/platform/monetisation is a candidate only after checking existing billing modules. Keep commercial records outside customer Books. Vimbai's own accounting Book records its revenue through the ordinary ledger, not by editing customers' records.

## Contracts
Offer has stable ID, audience, currency, price, billing interval, jurisdiction, effective dates and feature grants. Entitlement decision takes account/organisation, feature, scope, time and verified subscription state. Authorization and entitlement are independent: both must allow a paid operation. Free access, trial, paid and sponsored access use the same policy evaluator. Preserve read/export and lawful retention after lapse.

## Operations
Price changes are versioned, never retroactive without agreed terms. Record taxes, discounts, refunds and chargebacks; reconcile provider settlement to merchant invoices. No hard-coded global prices or mandatory single provider. Proposed modules catalogue, pricing, billing, entitlements, referrals and partners share IDs and events; API server composes them, Flutter consumes read-only offers and grants.

## Acceptance
Test duplicate events, expiry, downgrade, refund, concurrent grants, tenant isolation and offline grace. No paid feature bypasses ledger integrity. Measure contribution margin after provider fees, support and infrastructure; validate willingness to pay in pilot.
