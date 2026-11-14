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

7. **Cursor pagination is implemented, opt-in, with one stated ceiling.** A caller who sends no paging parameters gets
   exactly what they got before, which is how it landed in v1 without breaking anyone. A caller who sends `limit` or
   `cursor` gets `items`, `has_more`, `next_cursor`, `count` and `total`; a list that answers its rows under a name
   (`{"rates": [...], "count": 3}`) keeps that name as well. A cursor the engine did not issue answers 422, and a
   nonsensical `limit` answers 422 rather than being guessed at.

   The ceiling: the layer windows the rows the handler produced, and every list handler already caps what it reads. So
   `has_more` is true when the engine had more rows to give, and paging beyond that cap is a matter of raising the
   handler's cap - it is not something a caller can ask past. Seeking from a cursor inside the store, rather than
   windowing the answer, is the remaining work; the shape is in place so a client can be written against it now.

Every convention ADR-004 states is now implemented or stated as not. None is hidden behind a claim that it holds. Anything a client is told here is
verified by the tests named in `docs/compliance/financial-integrity.md` where it touches the ledger, and by
`cargo run -p xtask -- openapi --check` where it touches an address.
