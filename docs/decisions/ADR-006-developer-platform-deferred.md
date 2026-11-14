# ADR-006: The developer platform is deferred

*Status:* accepted, 2026-11-14. Deferred, not cancelled.

## Context

The proposal includes a developer platform domain (application registration, OAuth clients, scoped permissions, usage,
sandbox, extensions, certification) plus a public portal, SDKs in five languages, examples and certification documents.

Two facts argue for waiting. It is technically blocked: every piece of it keys on an organisation boundary the codebase
does not have yet, and on API contracts that are not written. And it is commercially early: a portal and SDK set cost
maintenance every month, and pay off only when there are third parties to serve.

## Decision

1. No developer portal, no SDK generation, no sandbox, no extension registry, no certification programme in this roadmap.
2. The parts of it that everything else needs are built anyway, under their own phases: the organisation boundary
   (P2) and versioned contracts with their conventions (ADR-004, P6).
3. `sdk/` stays empty rather than being filled with generated clients for an API that is still settling.

## Trigger to revisit

Either of these is evidence rather than a guess:

1. A customer or partner asks for API credentials to build against Vimbai, and the integration cannot be done inside the
   Flutter app.
2. An integration requires a keyed server-to-server call, a signed webhook, or an application identity that is not a
   Vimbai user.

On that trigger, build the smallest slice first: application registration, scoped keys, one sandbox tenant, signed
webhooks. Not the portal, not the SDK matrix, not certification.

## Consequences

- Integration stories are told through examples in the repository and the published contracts, not through a portal.
- Because contracts are being written now, deferring the platform does not defer the work it depends on. That is the
  point of the ordering.
