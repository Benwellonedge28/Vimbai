# Offline-first and synchronization
Status: proposed extension of SYNC.md and LOCAL-RUNTIME.md.

## Existing documented baseline
Sync server narrows engine router to health, identity and Books. Entries use opaque encrypted payloads, per-Book sequence, entry-ID deduplication and optional record version. Conflicting accounting updates are refused; selected nonfinancial families currently allow last-writer-wins. Sync server cannot post journals.

## Proposed refinement
Separate encrypted replication acceptance from authoritative financial posting. A queued intent is not a posted entry; local authoritative posting and shared ledger reconciliation need an explicit trust/key/sequence protocol before enabling multi-device offline final posting. Never claim server validates opaque contents. Preserve immutable history and show pending/conflict states. Recheck current authorization on reconnect.

## Integration
Extend platform/offline-sync conflict.rs and existing Book push/pull handlers; Flutter core/offline owns durable queue and cursor; local runtime uses same kernel; sync server remains narrow. Persist entry ID, base revision, payload integrity metadata and acknowledgement. Advance cursor only after durable application of returned page. Encrypt local data and protect keys; define device revocation and recovery.

## Acceptance
Lost response, interrupted batch, duplicate replay, concurrent edits, stale authorization, corrupted payload, cursor pagination, key loss and device restart. No silent overwrite of financial records. Resolve conflicts with explicit user action or deterministic approved policy.
