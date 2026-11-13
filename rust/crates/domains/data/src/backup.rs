#![forbid(unsafe_code)]
//! encrypted-backup-service: the backups a Book took, the key each was sealed with, and the restores asked of them.
//!
//! What this service is honest about, and what the port keeps honest:
//!
//! * a backup is a RECORD of a backup, not the backup. No bytes are copied here: the size is zero because nothing was
//!   measured, and the file path names where a real archiver would have put the archive. A record claiming a size it
//!   never saw is the kind of fiction that makes a restore fail at the worst moment;
//! * the checksum is real. The shipped service made one up - a uuid dressed as a digest - which cannot detect anything.
//!   Here it is SHA-256 over the identity of the backup (its service, its type, the key it was sealed with and the
//!   moment it was taken), computed by the one hash implementation the platform has, so the same backup names the same
//!   digest twice and a changed one names a different digest;
//! * the backup type is one of full, incremental, differential, and anything else is refused as a bad request rather
//!   than quietly stored as "full". A backup labelled with a type nobody agreed on is worse than a refusal;
//! * a key id is minted when the caller names no key, so every backup carries the identity of the key it was sealed with.
//!   Losing that leaves a sealed archive nobody can open;
//! * a restore is refused unless the backup it names finished. Restoring from a backup that never completed is the one
//!   mistake that turns a bad day into a lost Book, so it is a bad request and not a best effort;
//! * a backup that is not there is missing, deleting answers what it deleted, and one Book never sees another's.

use serde_json::{json, Value};

use vimbai_common::hash::sha256_hex;
use vimbai_common::ids::service_id;
use vimbai_store::read::{number, text};
use vimbai_store::{BookScope, Graph, RecordKind, Records, StoreError};

/// A backup that was taken.
pub const BACKUP: RecordKind = RecordKind::new("EncryptedBackup", "OWNS_ENCRYPTED_BACKUP");
/// A restore that was asked of one.
pub const RESTORE: RecordKind = RecordKind::new("EncryptedRestore", "OWNS_ENCRYPTED_RESTORE");

/// The three backup shapes the service knows. Anything else is refused by name.
pub const TYPES: [&str; 3] = ["full", "incremental", "differential"];

fn stamp(payload: &Value) -> Value {
    match payload.get("now") {
        Some(stated) if !stated.is_null() => stated.clone(),
        _ => json!(vimbai_common::clock::moment()),
    }
}

/// `POST /backup`: record an encrypted backup of one service.
pub fn take_backup(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let service_name = text(payload, "service_name");
    if service_name.trim().is_empty() {
        return Err(vimbai_store::read::refused("service_name is required"));
    }
    let kind = match text(payload, "backup_type").as_str() {
        "" => "full".to_string(),
        named if TYPES.contains(&named) => named.to_string(),
        _ => {
            return Err(StoreError::BadRequest(format!(
                "Invalid backup type. Must be one of {TYPES:?}"
            )))
        }
    };
    let key = match text(payload, "encryption_key_id") {
        named if named.is_empty() => service_id(),
        named => named,
    };
    let id = match text(payload, "id") {
        named if named.is_empty() => service_id(),
        named => named,
    };
    let taken = stamp(payload);
    // the digest covers what identifies this backup, and is the one hash instead of a made-up one
    let digest = sha256_hex(format!("{service_name}|{kind}|{key}|{taken}").as_bytes());
    Records::new(graph, BACKUP).create(
        scope,
        &json!({
            "id": id,
            "service_name": service_name,
            "backup_type": kind,
            "status": "completed",
            "file_path": format!("/backups/{id}.enc"),
            "encryption_key_id": key,
            // nothing was copied, so nothing is claimed about its size
            "size_bytes": 0,
            "checksum": format!("sha256:{digest}"),
            "created_at": taken,
            "completed_at": taken,
            "created_by": text(payload, "created_by"),
        }),
    )
}

/// `GET /backups`: the Book's backups, narrowed by service and status when asked.
pub fn backups(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let wanted_service = text(payload, "service_name");
    let wanted_status = text(payload, "status");
    let rows = Records::new(graph, BACKUP).list(scope, 500)?;
    Ok(Value::Array(
        rows.into_iter()
            .filter(|row| wanted_service.is_empty() || text(row, "service_name") == wanted_service)
            .filter(|row| wanted_status.is_empty() || text(row, "status") == wanted_status)
            .collect(),
    ))
}

/// `GET /backups/{id}`: one backup.
pub fn backup(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    backup_id: &str,
) -> Result<Value, StoreError> {
    Records::new(graph, BACKUP).get(scope, backup_id)
}

/// `POST /backup/{id}/restore`: ask for a restore, which only a finished backup can answer.
pub fn restore(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    backup_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let held = backup(graph, scope, backup_id)?;
    if text(&held, "status") != "completed" {
        return Err(StoreError::BadRequest(
            "Backup is not in completed state".to_string(),
        ));
    }
    let taken = stamp(payload);
    Records::new(graph, RESTORE).create(
        scope,
        &json!({
            "id": if text(payload, "id").is_empty() { service_id() } else { text(payload, "id") },
            "backup_id": backup_id,
            "status": "completed",
            "restored_by": text(payload, "restored_by"),
            "created_at": taken,
            "completed_at": taken,
        }),
    )
}

/// `DELETE /backups/{id}`: forget a backup.
pub fn delete_backup(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    backup_id: &str,
) -> Result<Value, StoreError> {
    backup(graph, scope, backup_id)?;
    Records::new(graph, BACKUP).delete(scope, backup_id)?;
    Ok(json!({"deleted": true, "backup_id": backup_id}))
}

/// `GET /restores`: the restores this Book asked for.
pub fn restores(graph: &(impl Graph + ?Sized), scope: &BookScope) -> Result<Value, StoreError> {
    Ok(Value::Array(Records::new(graph, RESTORE).list(scope, 500)?))
}

/// The size a backup claims, in bytes. Zero means nothing was measured, not that nothing was saved.
pub fn size_claimed(backup_row: &Value) -> u64 {
    number(backup_row, "size_bytes") as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::MemoryGraph;

    fn talked() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    #[test]
    fn a_backup_is_recorded_with_a_real_digest_and_an_honest_size() {
        let graph = MemoryGraph::new();
        let made = take_backup(
            &graph,
            &talked(),
            &json!({"service_name": "accounting-service", "backup_type": "full",
                    "created_by": "user-1", "now": 1000.0}),
        )
        .expect("a backup");
        assert_eq!(made["status"], json!("completed"));
        assert_eq!(made["backup_type"], json!("full"));
        assert_eq!(made["created_at"], made["completed_at"]);
        assert_eq!(size_claimed(&made), 0);
        // no key was named, so one was minted: a sealed archive must name its seal
        assert!(!text(&made, "encryption_key_id").is_empty());
        let checksum = text(&made, "checksum");
        assert!(checksum.starts_with("sha256:"));
        assert_eq!(checksum.len(), "sha256:".len() + 64);

        // the same backup twice names the same digest; a different one names a different digest
        let again = take_backup(
            &graph,
            &talked(),
            &json!({"service_name": "accounting-service", "backup_type": "full",
                    "encryption_key_id": text(&made, "encryption_key_id"), "now": 1000.0}),
        )
        .expect("a backup");
        assert_eq!(text(&again, "checksum"), checksum);
        let other = take_backup(
            &graph,
            &talked(),
            &json!({"service_name": "accounting-service", "backup_type": "incremental", "now": 1000.0}),
        )
        .expect("a backup");
        assert_ne!(text(&other, "checksum"), checksum);

        // a type nobody agreed on is a bad request, not quietly "full"
        let refused = take_backup(
            &graph,
            &talked(),
            &json!({"service_name": "accounting-service", "backup_type": "partial"}),
        );
        assert!(matches!(refused, Err(StoreError::BadRequest(_))));
        assert!(take_backup(&graph, &talked(), &json!({})).is_err());
    }

    #[test]
    fn only_a_finished_backup_can_be_restored_and_nothing_crosses_books() {
        let graph = MemoryGraph::new();
        let made = take_backup(
            &graph,
            &talked(),
            &json!({"service_name": "cashbook-service", "now": 2000.0}),
        )
        .expect("a backup");
        let id = text(&made, "id");

        let restored = restore(
            &graph,
            &talked(),
            &id,
            &json!({"restored_by": "user-1", "now": 2100.0}),
        )
        .expect("a restore");
        assert_eq!(restored["status"], json!("completed"));
        assert_eq!(restored["backup_id"], json!(id));
        assert_eq!(restored["restored_by"], json!("user-1"));
        assert_eq!(
            restores(&graph, &talked())
                .expect("restores")
                .as_array()
                .map(Vec::len),
            Some(1)
        );

        // a backup that is not there is missing, from either door
        assert!(restore(&graph, &talked(), "nope", &json!({})).is_err());
        assert!(backup(&graph, &talked(), "nope").is_err());
        // and another Book cannot see, restore or delete it
        let elsewhere = BookScope::new("user-2", "book-2");
        assert!(restore(&graph, &elsewhere, &id, &json!({})).is_err());
        assert!(delete_backup(&graph, &elsewhere, &id).is_err());
        assert_eq!(
            backups(&graph, &elsewhere, &json!({}))
                .expect("backups")
                .as_array()
                .map(Vec::len),
            Some(0)
        );

        // the listing narrows by service and by status
        take_backup(
            &graph,
            &talked(),
            &json!({"service_name": "ledger-service"}),
        )
        .expect("a backup");
        let cashbook = backups(
            &graph,
            &talked(),
            &json!({"service_name": "cashbook-service"}),
        )
        .expect("backups");
        assert_eq!(cashbook.as_array().map(Vec::len), Some(1));
        let done = backups(&graph, &talked(), &json!({"status": "completed"})).expect("backups");
        assert_eq!(done.as_array().map(Vec::len), Some(2));
        let running = backups(&graph, &talked(), &json!({"status": "running"})).expect("backups");
        assert_eq!(running.as_array().map(Vec::len), Some(0));

        // deleting answers what went, and a second delete finds nothing
        assert_eq!(
            delete_backup(&graph, &talked(), &id).expect("a delete")["deleted"],
            json!(true)
        );
        assert!(delete_backup(&graph, &talked(), &id).is_err());
    }

    #[test]
    fn a_restore_is_refused_for_a_backup_that_never_finished() {
        let graph = MemoryGraph::new();
        // a backup left half-done, as a crashed archiver would leave it
        Records::new(&graph, BACKUP)
            .create(
                &talked(),
                &json!({"id": "half", "service_name": "accounting-service", "status": "running"}),
            )
            .expect("a stored backup");
        let refused = restore(&graph, &talked(), "half", &json!({}));
        assert!(matches!(refused, Err(StoreError::BadRequest(_))));
        assert_eq!(
            restores(&graph, &talked())
                .expect("restores")
                .as_array()
                .map(Vec::len),
            Some(0)
        );
    }
}
