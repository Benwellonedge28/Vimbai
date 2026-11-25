# Restore a Book with unsynced work after device failure

Baseline `728f9a22a5371f40fce7278c31566f8a074f2884`. Depends on 03 and ADR-007.

## Existing behavior

`apps/flutter/lib/features/books/data/book_sync_service.dart::enqueueEntry` queues shared-Book work, `syncBook` synchronizes it, and `bookKeyFor` reads the key used to access that Book. Audit the Flutter accounting queue and attachment locations separately from the shared-Book queue before claiming coverage. `rust/crates/domains/data/src/backup.rs::take_backup` writes a record with a checksum of metadata and a zero file size. Its `restore` records a restore request as completed. Those paths do not copy or restore Book bytes at this baseline. Keep server database recovery and on-device recovery as separate tested procedures.

## File format and export

Add a recovery service under `apps/flutter/lib/features/books/data/` to coordinate a point-in-time manifest and the supported Book store readers. Record format version, Book identity, snapshot cursor, stable operation IDs, per-table counts, attachment hashes, and encryption parameters. Encrypt the manifest and content with authenticated encryption. Generate a separate recovery key and let the user record it away from the device. Existing Book key wrapping may support sharing; an export that requires the original device key cannot serve as the recovery copy.

Export the actual local records, pending entries, outbox intents, and completed attachment bytes covered by the manifest. Keep credentials, refresh tokens, and unrelated Books out of the archive. Derive the data from consistent read transactions without copying open SQLite files. Support Android’s file picker for a user-selected folder on removable portable storage and a file destination for Windows. Write a new version, reopen it, authenticate its contents and verify the manifest before advancing the last verified backup marker. Preserve the previous verified copy if the destination disappears or storage fills.

## Restore and replay

The new client reads the header, checks version and size bounds, authenticates and decodes into a staging store, checks ownership and references, then activates the staged Book atomically. Provide a local recovery mode that needs the backup and recovery key to open archived data when offline. Require online verification before reattaching an organisation Book or submitting its queued operations. Keep the restored operation IDs and payload digests. On reconnect, replay through the normal idempotency and access gates. If an operation was already accepted, reconcile against the existing server receipt. A changed payload or revoked right leaves the operation visible as requiring review. Never replace an occupied Book silently.

## Errors and compatibility

A wrong key or tampered archive yields “This backup could not be unlocked or verified. Your current Books are safe.” A removed card or interrupted write leaves the last verified timestamp unchanged. An unsupported archive version shows the minimum compatible app version where known. Existing Books need no conversion before the first export. Legacy backup metadata records remain historical and must not display as verified portable backups. Store-move tooling remains a separate administrator path.

## Acceptance tests

- `offline_expense_and_attachment_restore_from_sd_without_original_device` tests airplane mode, physical card removal and a fresh install.
- `replaying_a_restored_outbox_posts_each_operation_once` runs with unsent and already accepted messages.
- `corrupt_wrong_key_or_interrupted_write_preserves_the_previous_book` tests atomic rollback.
- `revoked_member_can_neither_reattach_nor_push_restored_changes` covers organisation access on reconnect.
- Done when two physical Android devices complete the demonstrated recovery and the restored trial balance and queued operation IDs match the pre-failure evidence.
