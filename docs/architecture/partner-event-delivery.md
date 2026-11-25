# Deliver one partner event safely after the integration trigger

Source baseline `2df3a7685b1109756fad0cca4d7b648b25790bdb`. Conditional work under ADR-006. Start after an identified customer or partner needs application identity or signed server-to-server delivery and the owner records the trigger. Requires 03 and production operations from 06.

## Existing surface and trigger record

`rust/crates/platform/webhooks/src/webhooks.rs` exposes `register_endpoint`, `dispatch_event`, `signature` and an injected `Sender`. The current dispatcher records an attempt after sending, permits a missing secret and treats response codes below 400 as delivered. `rust/apps/api-server/src/records.rs` mounts the webhook endpoints, and `rust/apps/api-server/tests/webhooks_http.rs` exercises that surface. `rust/crates/platform/integrations/src/graphql.rs` supplies a separate query surface. Its existence does not authorize app credentials for Book records.

## First partner contract

Record the named integration, event it needs, authorised tenant and purpose, data fields, expected rate, and test endpoint. Start with one event such as an invoice posting notice containing event ID, Book ID, opaque document ID, type, occurred time and schema version. Leave financial line items and counterparty identifiers out unless the partner has a justified scope and explicit approval. Bind an application registration to one organisation, one allowed Book or Book set, event types and an expiry. Use a sandbox tenant and credentials distinct from production.

Write an event to a durable outbox in the same transaction as the subscribed state change, then deliver through the existing worker pattern. Enforce one delivery identity per endpoint and event. Authenticate exact payload bytes and bind event ID, timestamp and signing-key ID into a versioned signature. Store signing secrets by reference, record rotation and revocation, and keep secrets and raw payloads out of general logs. The sender must enforce HTTPS, approved destinations and restricted egress, including after resolution; disable redirects. Only 2xx acknowledges a delivery. Retry bounded transient failures with jitter and record terminal failures for an authorised operator. Replay preserves the event ID so receivers can deduplicate.

## Legacy endpoints and failure behavior

Preserve historical delivery evidence. Treat legacy endpoints as inactive for the new partner mode until their owner confirms the destination and rotates onto the new signing contract. Reject a destination or event outside scope with a specific reason. When a retry limit is reached, show the operator the last result and a single audited replay action. No write or payout APIs are included in this package. A portal, generated SDKs, extension registry and certification remain governed by ADR-006.

## Acceptance tests

- `a_committed_invoice_produces_one_durable_partner_event` covers failed posting and crash-before-send.
- `receiver_retry_uses_the_same_event_identity_and_signed_payload` covers an interrupted acknowledgment and replay.
- `scope_revocation_stops_unsent_deliveries` checks older queued events.
- `an_unapproved_endpoint_is_never_called` verifies destination and redirect policy with a controlled transport.
- Done when a single partner validates the test event in a sandbox, a production owner verifies delivery and revocation, and the commercial or operational trigger is linked in the review record.
