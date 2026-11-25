# Keep business documents as verifiable evidence

Source baseline `2df3a7685b1109756fad0cca4d7b648b25790bdb`. Depends on the Book permissions in 03 and supplies the attachment contract for 05 and 19.

## Existing surface

`rust/crates/platform/documents/src/documents.rs` provides upload, download, listing and linkage. `upload_document` hashes the base64 text, checks its encoded length and uses the submitted `file_size`. Its duplicate check reads a bounded list. `link_document` checks the document under the Book scope before storing the submitted target fields. Existing behavior is exercised by `rust/apps/api-server/tests/documents_http.rs` and `rust/apps/api-server/tests/document_http.rs`.

## Storage and lifecycle

Separate business records from their evidence objects. An invoice owns sales status, amounts and posting references. Its PDF, source image and OCR result are evidence revisions. Reuse the existing domain ownership and add document revisions, immutable byte metadata, and Book-scoped evidence links under the platform documents crate. Dispatch target lookup through an explicit registry of supported kinds. The server resolves both document and target in the same Book and checks the actor’s linking capability.

Decode the incoming base64 once at the boundary, reject invalid bytes or inconsistent file metadata, and enforce a decoded size limit verified by the server. Compute digest and length from decoded bytes. Deduplicate with a unique key scoped to Book and digest. Reusing bytes should allow a distinct evidence relationship without copying the file. Commit evidence metadata only once its bytes can be read back and verified. Preserve historical versions when replacing or redacting evidence, with an audit event and retention policy. Downloads require current Book scope and permission. Never use a filename or client-supplied content type as authorization. OCR suggestions are reviewable metadata and do not create postings.

## Compatibility and failure

Use a versioned digest scheme when migrating existing encoded blobs, retaining IDs and legacy checksums. Quarantine unreadable rows and report missing links by ID. The list API exposes availability and revision metadata; recovery and store export carry the evidence manifest and verified bytes. When a write or target lookup fails, return the reason before attaching anything. Preserve posted financial records when evidence is missing.

## Acceptance tests

- `upload_uses_decoded_bytes_for_length_and_idempotency` checks legacy and equivalent encodings and a concurrent upload.
- `cross_book_evidence_link_is_refused_without_a_write` covers both linking and download.
- `replacing_evidence_keeps_the_approved_revision_readable` checks audit history and access.
- `attachment_manifest_round_trips_with_its_book` compares hashes after export and recovery.
- Done when an invoice and supplier bill can be read with their original evidence on a restored Book, with identical posted totals.
