# Subscriptions, billing and entitlements
Status: proposed. Existing subscription implementation not verified.

## Model
Subscription belongs to a billing account; coverage explicitly names user, household or organisation and eligible Books. Plans target personal, household, sole trader and institutional needs without creating separate engines. Price versions specify currency, interval, tax treatment and renewal terms. States: pending, trial, active, past due, cancelled, expired; refunds and disputes are separate financial events.

## Flow
Create checkout intent → external provider confirms → verify signed event and query provider when uncertain → atomically record payment and update subscription → publish entitlement change. Browser redirect is never payment proof. Renewal, cancellation and expiry use durable worker jobs; idempotency keys prevent duplicate grants. No automatic recurring debit without provider support and consent.

## Integration
Monetisation owns subscription and entitlement contracts; existing identity/tenancy resolves scope; worker processes expiry; gateway and engine enforce feature access; Flutter caches signed short-lived entitlement snapshot. Offline grace is configurable and cannot authorize cross-tenant access. Preserve export and read access on lapse.

## Tests
Duplicate/out-of-order callbacks, partial payment, refund, grace expiry, plan migration, cancellation, concurrent renewal, clock skew and provider outage. Reconcile invoices, payments and entitlement changes daily; alert on mismatch.
