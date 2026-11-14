# Request conventions: what is implemented, and what is still only written down

ADR-004 states eight conventions. Six are in the engine and a client can rely on them today. Two are stated in ADR-004
and *not* implemented, and this page records that rather than letting a document read as a description of code.

## Implemented, and relied on

1. **The one error envelope.** Every refusal answers `{"error": "<string>"}` with an HTTP status carrying the class:
   400 for a request the caller can fix, 401 unauthenticated, 403 out of scope, 404 missing, 409 conflict, 422 a
   rejected value. The envelope is emitted in one place (`records::refused` and the identity gate), so a second shape
   cannot appear beside it unnoticed.
2. **Scope in headers.** `X-User-Id` and `X-Book-Id` identify the caller and the Book. The gateway sets them from the
   token and ignores what a client sends; the engine's tenancy gate refuses a request for a Book the caller cannot open
   with 403 rather than answering an empty listing.
3. **Money as a string.** Amounts travel as JSON strings holding an exact decimal and are `common::Money` end to end.
   A number where a string belongs is treated as a defect, which is what `wire()` publishing `to_fixed(6)` is for.
4. **Timestamps as ISO 8601 instants**, in responses and in events. There is one clock reading (`common::clock::moment`)
   and the field types come from it.
5. **Base64 for binary and encrypted blobs** in JSON transport, since a JSON string is the only way a byte array survives
   a round trip without a schema of its own.
6. **Bounded reads are bounded by the server.** Where a collection is inherently finite - an account's postings, a
   period's entries - the engine caps the limit rather than trusting the caller, and the cap is stated in the handler.

7. **Idempotency keys.** A write may carry `Idempotency-Key`. The first request with a key is claimed, run and answered;
   a repeat with the same key and the same method, path and body replays that answer (including a refusal); a repeat with
   a different request answers 422; a repeat while the first is still running answers 409 and says so; a write with no key
   is untouched. Claims are platform records keyed by caller, Book and key, so two callers cannot collide, and the worker
   removes the ones older than a day. Eight tests hold it, in `rust/apps/api-server/tests/idempotency_http.rs`, including
   one that goes through the assembled engine rather than the layer alone.

## Stated in ADR-004 and not yet implemented

7. **Cursor pagination.** `items`, `has_more` and `next_cursor` are not what the engine returns: a list answers a plain
   JSON array, and the caller pages with `limit` and `offset` where the handler supports it. The engine is therefore
   consistent with itself and not with ADR-004. Closing this means changing every list handler and every client that
   reads an array, which is a `v2` change if it is done by shape and a `v1` change only if it is added beside the array.
8. **Cursor pagination**, described above, is the only convention left unimplemented.

It is not hidden behind a claim that the convention holds. Anything a client is told here is
verified by the tests named in `docs/compliance/financial-integrity.md` where it touches the ledger, and by
`cargo run -p xtask -- openapi --check` where it touches an address.
