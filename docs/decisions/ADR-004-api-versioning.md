# ADR-004: API versioning and the conventions a client can rely on

*Status:* accepted, 2026-11-14. Implementation completes with roadmap P6.

## Context

The engine publishes 1529 addresses, reached through the Go gateway. Those addresses were inherited from the Python
services, so they carry that estate's habits: paths without a version, errors as free-form strings, and no agreed
pagination. Flutter is the only client and its data layer is about to be restructured, so the conventions have a short
window in which they are cheap to fix and a long window afterwards in which they are expensive.

## Decision

1. *Paths stay where clients already point.* The published address is a contract. A retired service's addresses keep
   working at the same path, answered by the engine. Nothing is re-prefixed cosmetically, because a Flutter release in
   the field cannot be coordinated with a rename.
2. *Versioning is by URL prefix, introduced with the first published contract.* `contracts/openapi/v1/` describes
   `/v1/...`. The engine continues to answer the unversioned addresses as aliases; a new version is a new prefix, never a
   silent change to an existing one.
3. *Additive by default.* Adding a field, an endpoint or an optional parameter is not a version change. Removing a field,
   renaming one, changing a type, or tightening validation is. The second kind requires a new version and a deprecation
   window of at least one client release.
4. *One error envelope.* Every error response is a JSON object with `error` carrying a human-readable message, and the
   HTTP status carrying the class of failure: 400 for a request the caller can fix, 401 unauthenticated, 403 forbidden
   or out of scope, 404 for a missing record, 409 for a conflict with existing state, 422 for a rejected value. The
   engine already answers `{"error": "..."}`; the contract makes it the only shape.
5. *Scope travels in headers.* `X-User-Id` and `X-Book-Id` identify the acting user and the book a call is scoped to.
   The gateway sets them from the token and ignores whatever a client sends; the engine refuses a book-scoped read with no
   book rather than returning an empty list, because an empty list is a wrong answer that looks like a right one.
6. *Collections are paginated or explicitly bounded.* Where a collection can grow without limit, the response carries
   `items`, `has_more` and `next_cursor`; the caller passes `cursor` and `limit`. Where a read is inherently bounded
   (an account's postings, a period's entries) the bound is documented and enforced by the server, not left to the
   caller's goodwill.
> **Implementation status (P6):** all eight conventions are implemented. Convention 6 (cursor pagination) is opt-in by
> request, so no existing client's shape changed: send `limit` or `cursor` and the answer carries `items`, `has_more` and
> `next_cursor`; send neither and the answer is what it always was. It windows the rows a handler produced, so it cannot page
> past that handler's own cap - that ceiling is stated in `contracts/compatibility/request-conventions.md`. Convention 7
> (idempotency keys) is implemented in one layer beside the tenancy gate.

7. *Writes that must not repeat carry an idempotency key.* A retried write with the same key returns the first result
   rather than performing the operation twice. Payments and postings need this; the mechanism is defined once, in the
   application layer.
8. *Payload conventions.* Money is a JSON string holding an exact decimal, never a float. Binary and encrypted blobs are
   base64 strings. Timestamps are ISO 8601. Identifiers are opaque strings.

## Consequences

- The contract gate (P6) checks both directions: every published address appears in a contract, and every contract path is
  answered. A route nobody documented and a document nobody serves are the same defect.
- Until P6 lands, these conventions are stated here and applied by hand. That is honest but weak, which is why the gate
  is part of the same phase rather than a later one.
- Clients can regenerate a typed model from the contract instead of hand-writing one per screen.
