# Grant one Vimbai subscription across Android, Windows and web

Baseline `728f9a22a5371f40fce7278c31566f8a074f2884`. Depends on 03 and the first signed release from 06. Subject to ADR-005 and ADR-006.

## Existing boundary

`rust/crates/domains/invoicing/src/billing.rs::create_plan` and `subscribe` take a `BookScope`. They model billing inside a Book and are covered by `rust/apps/api-server/tests/billing_http.rs`. Vimbai’s own subscription belongs outside that customer accounting model. The store already distinguishes `Records` and `PlatformRecords` in `rust/crates/adapters/database/src/lib.rs`. Add commercial policy in a new application module and use platform-scoped records with authorization by billing subject. Keep customer Book exports free of provider references and Vimbai sales data.

## Domain and integration

A billing subject is either a Vimbai user or an organisation with a named billing admin. Persist immutable plan version and provider SKU mappings, a pending purchase with a unique reference, a provider transaction record, and an entitlement with subject, features, status, effective time and end time. Amount and currency come from the version purchased. Record signed or verified provider evidence internally with restricted access. Enforce unique provider event and purchase identifiers. Apply state transitions and replay protection in one transaction. Use provider effective time and authoritative current state to resolve out-of-order renewals, refunds and cancellations.

Add proposed endpoints `GET /commercial/entitlements`, `POST /commercial/checkout`, and separately authenticated provider callbacks in the API server. The data contract returns active features, status, expiry, billing source and a policy version. Website checkout starts a Paynow payment, confirms it from the provider server, then grants the entitlement. A browser return displays pending until verification. The initial mobile-money plan is prepaid for a defined period. Android Play purchases link to the signed-in Vimbai subject, are verified and acknowledged server-side, and update from Play notifications. Windows and web read the same Vimbai entitlement. Keep Play purchase flows separate from website acquisition according to the distribution policy used at release.

## Expiry, authorization and migration

Provide a short-lived signed offline entitlement for supported features under a documented grace policy. Record changes while offline under the normal Book permissions and reconcile entitlements on reconnect. Expiry must preserve access to existing records and recovery/export rights under the product policy. Explain pending, expired, account mismatch and refunded states distinctly. A duplicate event may return success after confirming its effect is already present. Existing Book billing records are left in their current model. Existing Vimbai accounts are provisioned with the default plan once and remain usable while a provider is unavailable.

## Acceptance tests

- `verified_play_purchase_unlocks_the_same_subject_on_web_and_windows` verifies server-side account association.
- `paynow_return_without_verified_payment_never_grants_access` covers abandoned and pending checkout.
- `duplicate_and_out_of_order_provider_events_converge` checks retry, renewal and refund ordering.
- `plan_expiry_preserves_book_read_and_recovery` checks account and organisation subjects.
- Done when the test and production provider modes are separated, sandbox reconciliation matches an independent transaction report, and release tests show one paid entitlement on all supported clients.
