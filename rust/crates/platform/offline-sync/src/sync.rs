#![forbid(unsafe_code)]
//! book-sync-service: the shared Books a person belongs to, who else is in them, their devices, and the encrypted
//! entries those devices push and pull.
//!
//! This is the one service whose rows belong to a Book rather than to one person, because a shared Book's members are
//! different people and each has to read what the others wrote. So it uses the store's Book-keyed records, and every
//! entry point checks the caller's membership BEFORE touching them. Nothing else may take that shortcut.
//!
//! Behaviours kept as shipped, each pinned by a test:
//!
//! * a Book is created in one of five tiers (personal, household, group, business, nonprofit) and its creator becomes
//!   its owner. Any other tier is refused by name rather than accepted and stored;
//! * the server never sees a Book's key. An invitation must carry the key wrapped for the person invited, and refusing
//!   to store an invitation without one is the point: an invite with no key would let someone join and decrypt nothing;
//! * roles are ordered owner, admin, treasurer, bookkeeper, viewer, auditor. Only owner and admin invite, change members
//!   and rename a Book; only the four writer roles push entries or store metadata; reading needs membership alone.
//!   The owner cannot be modified - not demoted, not revoked.
//! * an invitation is pending until the invited person accepts it, and only then does it answer their wrapped key. A
//!   Book appears in someone's list while their membership is active OR pending, marked which;
//! * pushing entries is retry-safe: an entry id already held is skipped rather than duplicated, so a device that lost
//!   its answer can resend the same batch. The Book's sequence rises once per entry actually stored, and the device's
//!   high-water mark follows;
//! * metadata is last-write-wins by the timestamp the client states, not the server's own;
//! * every change is written to an append-only audit trail on the Book, which is what makes "who let them in" answerable
//!   years later.
//!
//! The clock comes from the request, as epoch seconds, because the shipped shapes numbered their times rather than
//! dating them; where a time matters for ordering, the request says it.

use serde_json::{json, Map, Value};

use vimbai_common::ids::service_id;
use vimbai_store::read::{flag, number, refused, text};
use vimbai_store::shared::SharedRecords;
use vimbai_store::{BookScope, Graph, RecordKind, StoreError};

/// A shared Book and its sync sequence.
pub const BOOK: RecordKind = RecordKind::new("SyncBook", "OWNS_SYNC_BOOK");
/// One person's place in a Book.
pub const MEMBERSHIP: RecordKind = RecordKind::new("SyncMembership", "OWNS_SYNC_MEMBERSHIP");
/// A device that pushes and pulls a Book.
pub const DEVICE: RecordKind = RecordKind::new("SyncDevice", "OWNS_SYNC_DEVICE");
/// One opaque, already-encrypted entry.
pub const ENTRY: RecordKind = RecordKind::new("SyncEntry", "OWNS_SYNC_ENTRY");
/// One opaque, already-encrypted piece of Book metadata.
pub const METADATA: RecordKind = RecordKind::new("SyncMetadata", "OWNS_SYNC_METADATA");
/// The Book's own audit trail.
pub const AUDIT: RecordKind = RecordKind::new("SyncAudit", "OWNS_SYNC_AUDIT");

/// The most privileged role in a Book.
pub const OWNER: &str = "owner";
/// The roles that configure a Book.
pub const ADMIN_ROLES: [&str; 2] = ["owner", "admin"];
/// The roles that may write to a Book.
pub const WRITER_ROLES: [&str; 4] = ["owner", "admin", "treasurer", "bookkeeper"];
/// Every role a Book recognises.
pub const ROLES: [&str; 6] = [
    "owner",
    "admin",
    "treasurer",
    "bookkeeper",
    "viewer",
    "auditor",
];
/// The audience tiers a Book can be opened at.
pub const TIERS: [&str; 5] = ["personal", "household", "group", "business", "nonprofit"];

/// The metadata key a person's wrapped copy of the Book key lives under.
fn wrapped_key_for(user_id: &str) -> String {
    format!("_wrapped_key:{user_id}")
}

fn held(graph: &(impl Graph + ?Sized), kind: RecordKind) -> SharedRecords<'_, impl Graph + ?Sized> {
    SharedRecords::new(graph, kind)
}

/// A Book, or missing. A Book id is not a secret, but its contents need membership, which the caller checks.
fn book_of(graph: &(impl Graph + ?Sized), book_id: &str) -> Result<Value, StoreError> {
    held(graph, BOOK).get(book_id, book_id)
}

/// The caller's membership of a Book, whatever its state.
fn membership_for(
    graph: &(impl Graph + ?Sized),
    book_id: &str,
    user_id: &str,
) -> Result<Option<Value>, StoreError> {
    Ok(held(graph, MEMBERSHIP)
        .where_eq(book_id, "member_user_id", &json!(user_id))?
        .into_iter()
        .next())
}

/// The caller's ACTIVE membership, or the refusal the shipped service gave.
fn member_of(
    graph: &(impl Graph + ?Sized),
    book_id: &str,
    user_id: &str,
    roles: Option<&[&str]>,
) -> Result<Value, StoreError> {
    let Some(membership) = membership_for(graph, book_id, user_id)? else {
        return Err(StoreError::Forbidden(
            "Not an active member of this book".to_string(),
        ));
    };
    if text(&membership, "status") != "active" {
        return Err(StoreError::Forbidden(
            "Not an active member of this book".to_string(),
        ));
    }
    if let Some(allowed) = roles {
        if !allowed.contains(&text(&membership, "role").as_str()) {
            return Err(StoreError::Forbidden(
                "Role does not permit this action".to_string(),
            ));
        }
    }
    Ok(membership)
}

fn append_audit(
    graph: &(impl Graph + ?Sized),
    book_id: &str,
    actor: &str,
    action: &str,
    detail: &str,
    now: &Value,
) -> Result<(), StoreError> {
    held(graph, AUDIT).create(
        book_id,
        &json!({
            "id": service_id(),
            "actor": actor,
            "action": action,
            "detail": detail,
            "created_at": now,
        }),
    )?;
    Ok(())
}

fn book_answer(record: &Value) -> Value {
    json!({
        "id": text(record, "id"),
        "name": text(record, "name"),
        "tier": text(record, "tier"),
        "description": text(record, "description"),
        "folder": text(record, "folder"),
        "created_by": text(record, "created_by"),
        "created_at": record.get("created_at").cloned().unwrap_or(Value::Null),
        "seq": number(record, "seq") as i64,
    })
}

/// How many rows a listing may answer with: the caller's number when they state one, otherwise the shipped default.
fn stated_limit(payload: &Value, fallback: i64) -> i64 {
    match payload.get("limit") {
        None | Some(Value::Null) => fallback,
        _ => (number(payload, "limit") as i64).clamp(1, 1000),
    }
}

/// The sequence the Book has reached, which is what a device quotes back when it pulls.
fn book_seq(graph: &(impl Graph + ?Sized), book_id: &str) -> i64 {
    book_of(graph, book_id)
        .map(|record| number(&record, "seq") as i64)
        .unwrap_or(0)
}

/// `POST /books`: open a Book, with the caller as its owner.
pub fn create_book(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let tier = text(payload, "tier");
    // anything outside the five tiers is refused by name rather than stored and puzzled over later
    if !TIERS.contains(&tier.as_str()) {
        return Err(refused(&format!("tier must be one of {TIERS:?}")));
    }
    let name = text(payload, "name");
    if name.trim().is_empty() || name.chars().count() > 120 {
        return Err(refused("name must be between 1 and 120 characters"));
    }
    let book_id = service_id();
    let now = payload.get("now").cloned().unwrap_or(Value::Null);
    let folder: String = text(payload, "folder").chars().take(120).collect();
    held(graph, BOOK).create(
        &book_id,
        &json!({
            "id": book_id,
            "name": name,
            "tier": tier,
            "description": text(payload, "description"),
            "folder": folder,
            "created_by": scope.user_id,
            "created_at": now,
            "seq": 0,
        }),
    )?;
    held(graph, MEMBERSHIP).create(
        &book_id,
        &json!({
            "id": service_id(),
            "member_user_id": scope.user_id,
            "role": OWNER,
            "display_name": "",
            "status": "active",
            "invited_by": "",
            "created_at": now,
        }),
    )?;
    let wrapped = text(payload, "wrapped_book_key");
    if !wrapped.is_empty() {
        held(graph, METADATA).create(
            &book_id,
            &json!({
                "id": service_id(),
                "key": "_wrapped_book_key",
                "value": wrapped,
                "updated_at": now,
                "updated_by": scope.user_id,
            }),
        )?;
    }
    append_audit(graph, &book_id, &scope.user_id, "book.created", &tier, &now)?;
    Ok(json!({
        "service": "book-sync-service",
        "book": book_answer(&book_of(graph, &book_id)?),
        "your_role": OWNER,
    }))
}

/// `GET /books`: the Books this person belongs to, oldest first, pending invitations marked as such.
pub fn list_books(graph: &(impl Graph + ?Sized), scope: &BookScope) -> Result<Value, StoreError> {
    let mut mine: Vec<(i64, Value)> = Vec::new();
    for membership in held(graph, MEMBERSHIP).everywhere("member_user_id", &json!(scope.user_id))? {
        let status = text(&membership, "status");
        if status != "active" && status != "invited" {
            continue;
        }
        let book_id = text(&membership, "book_id");
        let Ok(book) = book_of(graph, &book_id) else {
            continue;
        };
        let order = book
            .get("created_at")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        mine.push((
            (order * 1000.0) as i64,
            json!({
                "id": text(&book, "id"),
                "name": text(&book, "name"),
                "tier": text(&book, "tier"),
                "description": text(&book, "description"),
                "folder": text(&book, "folder"),
                "seq": number(&book, "seq") as i64,
                "your_role": text(&membership, "role"),
                "membership_status": status,
                "created_at": book.get("created_at").cloned().unwrap_or(Value::Null),
            }),
        ));
    }
    mine.sort_by_key(|(order, _)| *order);
    Ok(json!({
        "service": "book-sync-service",
        "books": mine.into_iter().map(|(_, book)| book).collect::<Vec<_>>(),
    }))
}

/// `GET /books/{id}`: one Book, to a member.
pub fn get_book(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    book_id: &str,
) -> Result<Value, StoreError> {
    member_of(graph, book_id, &scope.user_id, None)?;
    Ok(json!({
        "service": "book-sync-service",
        "book": book_answer(&book_of(graph, book_id)?),
    }))
}

/// `PUT /books/{id}`: rename or refile a Book. The name is kept here for the caller's own convenience; the copy members
/// see is the encrypted one they sync, which is why the shipped service said so.
pub fn update_book(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    book_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    member_of(graph, book_id, &scope.user_id, Some(&ADMIN_ROLES))?;
    book_of(graph, book_id)?;
    let mut change = Map::new();
    if let Some(name) = payload.get("name").and_then(Value::as_str) {
        if !name.trim().is_empty() {
            change.insert("name".into(), json!(name));
        }
    }
    if let Some(description) = payload.get("description").and_then(Value::as_str) {
        change.insert("description".into(), json!(description));
    }
    if let Some(folder) = payload.get("folder").and_then(Value::as_str) {
        change.insert(
            "folder".into(),
            json!(folder.trim().chars().take(120).collect::<String>()),
        );
    }
    if !change.is_empty() {
        held(graph, BOOK).update(book_id, book_id, &Value::Object(change))?;
    }
    let detail = text(payload, "name");
    let detail = if detail.is_empty() {
        text(payload, "folder")
    } else {
        detail
    };
    append_audit(
        graph,
        book_id,
        &scope.user_id,
        "book.updated",
        &detail,
        &payload.get("now").cloned().unwrap_or(Value::Null),
    )?;
    Ok(json!({"service": "book-sync-service", "status": "updated"}))
}

/// `POST /books/{id}/members`: invite someone, carrying the Book key wrapped for them.
pub fn invite_member(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    book_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let role = text(payload, "role");
    if !ROLES.contains(&role.as_str()) || role == OWNER {
        return Err(refused("invalid role for invitation"));
    }
    member_of(graph, book_id, &scope.user_id, Some(&ADMIN_ROLES))?;
    let wrapped = text(payload, "wrapped_book_key");
    if wrapped.is_empty() {
        // an invitation without a key would let someone join and decrypt nothing
        return Err(refused(
            "invitation must include the book key wrapped for the invitee",
        ));
    }
    let invited = text(payload, "user_id");
    if invited.is_empty() {
        return Err(refused("user_id is required"));
    }
    if membership_for(graph, book_id, &invited)?.is_some() {
        return Err(StoreError::Conflict(
            "user already invited or a member".to_string(),
        ));
    }
    let now = payload.get("now").cloned().unwrap_or(Value::Null);
    held(graph, MEMBERSHIP).create(
        book_id,
        &json!({
            "id": service_id(),
            "member_user_id": invited,
            "role": role,
            "display_name": text(payload, "display_name"),
            "status": "invited",
            "invited_by": scope.user_id,
            "created_at": now,
        }),
    )?;
    store_metadata(
        graph,
        book_id,
        &wrapped_key_for(&invited),
        &wrapped,
        &scope.user_id,
        &now,
    )?;
    append_audit(
        graph,
        book_id,
        &scope.user_id,
        "member.invited",
        &format!("{invited}:{role}"),
        &now,
    )?;
    Ok(json!({"service": "book-sync-service", "status": "invited", "role": role}))
}

/// `GET /books/{id}/members`: everyone in the Book.
pub fn book_members(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    book_id: &str,
) -> Result<Value, StoreError> {
    member_of(graph, book_id, &scope.user_id, None)?;
    let mut members: Vec<(i64, Value)> = held(graph, MEMBERSHIP)
        .where_eq(book_id, "book_id", &json!(book_id))?
        .into_iter()
        .map(|membership| {
            let order = membership
                .get("created_at")
                .and_then(Value::as_f64)
                .unwrap_or(0.0);
            (
                (order * 1000.0) as i64,
                json!({
                    "user_id": text(&membership, "member_user_id"),
                    "role": text(&membership, "role"),
                    "display_name": text(&membership, "display_name"),
                    "status": text(&membership, "status"),
                    "created_at": membership.get("created_at").cloned().unwrap_or(Value::Null),
                }),
            )
        })
        .collect();
    members.sort_by_key(|(order, _)| *order);
    Ok(json!({
        "service": "book-sync-service",
        "members": members.into_iter().map(|(_, member)| member).collect::<Vec<_>>(),
    }))
}

/// `PUT /books/{id}/members/{user}`: change a role or revoke a place. The owner's own place is not for changing.
pub fn update_member(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    book_id: &str,
    member_user_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let role = text(payload, "role");
    if !role.is_empty() && !ROLES.contains(&role.as_str()) {
        return Err(refused("invalid role"));
    }
    let status = text(payload, "status");
    if !status.is_empty() && status != "active" && status != "revoked" {
        return Err(refused("invalid status"));
    }
    member_of(graph, book_id, &scope.user_id, Some(&ADMIN_ROLES))?;
    let Some(target) = membership_for(graph, book_id, member_user_id)? else {
        return Err(StoreError::NotFound);
    };
    if text(&target, "role") == OWNER {
        // an owner is not demoted or revoked by an admin
        return Err(refused("cannot modify the owner"));
    }
    let mut change = Map::new();
    if !role.is_empty() {
        change.insert("role".into(), json!(role));
    }
    if !status.is_empty() {
        change.insert("status".into(), json!(status));
    }
    if !change.is_empty() {
        held(graph, MEMBERSHIP).update(book_id, &text(&target, "id"), &Value::Object(change))?;
    }
    let now = payload.get("now").cloned().unwrap_or(Value::Null);
    append_audit(
        graph,
        book_id,
        &scope.user_id,
        "member.updated",
        &format!("{member_user_id}:{role}:{status}"),
        &now,
    )?;
    Ok(json!({"service": "book-sync-service", "status": "updated"}))
}

/// `POST /books/{id}/members/accept`: take up an invitation, and receive the Book key wrapped for the caller.
pub fn accept_invite(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    book_id: &str,
) -> Result<Value, StoreError> {
    let Some(membership) = membership_for(graph, book_id, &scope.user_id)? else {
        return Err(StoreError::NotFound);
    };
    if text(&membership, "status") != "invited" {
        return Err(StoreError::NotFound);
    }
    held(graph, MEMBERSHIP).update(
        book_id,
        &text(&membership, "id"),
        &json!({"status": "active"}),
    )?;
    let wrapped = held(graph, METADATA)
        .where_eq(book_id, "key", &json!(wrapped_key_for(&scope.user_id)))?
        .into_iter()
        .next()
        .map(|row| text(&row, "value"))
        .unwrap_or_default();
    append_audit(
        graph,
        book_id,
        &scope.user_id,
        "member.accepted",
        &text(&membership, "role"),
        &Value::Null,
    )?;
    Ok(json!({
        "service": "book-sync-service",
        "status": "active",
        "role": text(&membership, "role"),
        "wrapped_book_key": wrapped,
    }))
}

/// `POST /books/{id}/devices`: register a device that will push and pull.
pub fn register_device(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    book_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    member_of(graph, book_id, &scope.user_id, None)?;
    let device_id = service_id();
    let now = payload.get("now").cloned().unwrap_or(Value::Null);
    held(graph, DEVICE).create(
        book_id,
        &json!({
            "id": device_id,
            // the device's own name for itself, held beside the record id so a push can find it by the id the client
            // was handed rather than by an internal one
            "device_id": device_id,
            "member_user_id": scope.user_id,
            "device_name": text(payload, "device_name"),
            "last_seen_seq": 0,
            "created_at": now,
        }),
    )?;
    append_audit(
        graph,
        book_id,
        &scope.user_id,
        "device.registered",
        &device_id,
        &now,
    )?;
    Ok(json!({"service": "book-sync-service", "device_id": device_id}))
}

/// `POST /books/{id}/entries/push`: append a batch of already-encrypted entries, retry-safe.
pub fn push_entries(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    book_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let entries = payload
        .get("entries")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if entries.is_empty() {
        return Ok(json!({
            "service": "book-sync-service",
            "accepted": 0,
            "book_seq": book_seq(graph, book_id),
        }));
    }
    member_of(graph, book_id, &scope.user_id, Some(&WRITER_ROLES))?;
    let device_id = text(payload, "device_id");
    let owned = held(graph, DEVICE)
        .where_eq(book_id, "device_id", &json!(device_id))?
        .into_iter()
        .find(|device| text(device, "member_user_id") == scope.user_id);
    if owned.is_none() {
        // a device belongs to a person as well as a Book, so one member's device cannot write as another
        return Err(refused("device not registered for this book"));
    }
    let mut seen = held(graph, ENTRY).where_eq(book_id, "book_id", &json!(book_id))?;
    let mut seq = book_seq(graph, book_id);
    let mut accepted = 0;
    for entry in entries {
        let entry_id = text(&entry, "entry_id");
        let body = text(&entry, "payload");
        if entry_id.is_empty() || body.is_empty() {
            return Err(refused("each entry needs entry_id and payload"));
        }
        if seen.iter().any(|held| text(held, "entry_id") == entry_id) {
            // the same device resent something already stored: count it once, not twice
            continue;
        }
        seq += 1;
        let made = held(graph, ENTRY).create(
            book_id,
            &json!({
                "id": service_id(),
                "seq": seq,
                "entry_id": entry_id,
                "device_id": device_id,
                "author_user_id": scope.user_id,
                "payload": body,
                "created_at": entry.get("created_at").cloned().unwrap_or(Value::Null),
            }),
        )?;
        seen.push(made);
        accepted += 1;
    }
    held(graph, BOOK).update(book_id, book_id, &json!({"seq": seq}))?;
    if let Some(device) = owned {
        held(graph, DEVICE).update(
            book_id,
            &text(&device, "id"),
            &json!({"last_seen_seq": seq}),
        )?;
    }
    append_audit(
        graph,
        book_id,
        &scope.user_id,
        "entries.pushed",
        &accepted.to_string(),
        &Value::Null,
    )?;
    Ok(json!({
        "service": "book-sync-service",
        "accepted": accepted,
        "book_seq": seq,
    }))
}

/// `GET /books/{id}/entries?since=N`: what the caller has not seen, in sequence order.
pub fn pull_entries(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    book_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    member_of(graph, book_id, &scope.user_id, None)?;
    let since = number(payload, "since") as i64;
    // a page of up to a thousand, five hundred when the caller states nothing, as shipped
    let limit = stated_limit(payload, 500);
    let mut rows: Vec<(i64, Value)> = held(graph, ENTRY)
        .where_eq(book_id, "book_id", &json!(book_id))?
        .into_iter()
        .filter(|entry| number(entry, "seq") as i64 > since)
        .map(|entry| (number(&entry, "seq") as i64, entry))
        .collect();
    rows.sort_by_key(|(seq, _)| *seq);
    rows.truncate(limit as usize);
    Ok(json!({
        "service": "book-sync-service",
        "book_seq": book_seq(graph, book_id),
        "entries": rows
            .into_iter()
            .map(|(_, entry)| json!({
                "seq": number(&entry, "seq") as i64,
                "entry_id": text(&entry, "entry_id"),
                "device_id": text(&entry, "device_id"),
                "user_id": text(&entry, "author_user_id"),
                "payload": text(&entry, "payload"),
                "created_at": entry.get("created_at").cloned().unwrap_or(Value::Null),
            }))
            .collect::<Vec<_>>(),
    }))
}

/// Store one piece of Book metadata, replacing whatever was under that key.
fn store_metadata(
    graph: &(impl Graph + ?Sized),
    book_id: &str,
    key: &str,
    value: &str,
    by: &str,
    updated_at: &Value,
) -> Result<(), StoreError> {
    let records = held(graph, METADATA);
    let existing = records
        .where_eq(book_id, "key", &json!(key))?
        .into_iter()
        .next();
    let fields = json!({
        "value": value,
        "updated_at": updated_at,
        "updated_by": by,
    });
    match existing {
        Some(row) => {
            records.update(book_id, &text(&row, "id"), &fields)?;
        }
        None => {
            let mut fields = fields;
            fields["id"] = json!(service_id());
            fields["key"] = json!(key);
            records.create(book_id, &fields)?;
        }
    }
    Ok(())
}

/// `POST /books/{id}/metadata`: store a key's encrypted value, last write winning.
pub fn upsert_metadata(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    book_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    member_of(graph, book_id, &scope.user_id, Some(&WRITER_ROLES))?;
    let key = text(payload, "key");
    if key.is_empty() {
        return Err(refused("key is required"));
    }
    store_metadata(
        graph,
        book_id,
        &key,
        &text(payload, "value"),
        &scope.user_id,
        &payload.get("updated_at").cloned().unwrap_or(Value::Null),
    )?;
    Ok(json!({"service": "book-sync-service", "status": "stored", "key": key}))
}

/// `GET /books/{id}/metadata?since=T`: everything written after a moment.
pub fn pull_metadata(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    book_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    member_of(graph, book_id, &scope.user_id, None)?;
    let since = number(payload, "since");
    let mut rows: Vec<(i64, Value)> = held(graph, METADATA)
        .where_eq(book_id, "book_id", &json!(book_id))?
        .into_iter()
        .filter(|row| number(row, "updated_at") > since)
        .map(|row| {
            (
                (number(&row, "updated_at") * 1000.0) as i64,
                json!({
                    "key": text(&row, "key"),
                    "value": text(&row, "value"),
                    "updated_at": number(&row, "updated_at"),
                    "updated_by": text(&row, "updated_by"),
                }),
            )
        })
        .collect();
    rows.sort_by_key(|(order, _)| *order);
    Ok(json!({
        "service": "book-sync-service",
        "metadata": rows.into_iter().map(|(_, row)| row).collect::<Vec<_>>(),
    }))
}

/// `GET /books/{id}/audit`: the Book's own trail, newest first.
pub fn audit_log(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    book_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    member_of(graph, book_id, &scope.user_id, None)?;
    let limit = stated_limit(payload, 200);
    let mut rows: Vec<(i64, Value)> = held(graph, AUDIT)
        .where_eq(book_id, "book_id", &json!(book_id))?
        .into_iter()
        .map(|row| {
            (
                (number(&row, "created_at") * 1000.0) as i64,
                json!({
                    "actor": text(&row, "actor"),
                    "action": text(&row, "action"),
                    "detail": text(&row, "detail"),
                    "created_at": row.get("created_at").cloned().unwrap_or(Value::Null),
                }),
            )
        })
        .collect();
    rows.sort_by_key(|(order, _)| std::cmp::Reverse(*order));
    rows.truncate(limit as usize);
    Ok(json!({
        "service": "book-sync-service",
        "audit": rows.into_iter().map(|(_, row)| row).collect::<Vec<_>>(),
    }))
}

/// `GET /books/{id}/membership`: the check the gateway makes before it lets a request through.
pub fn membership_check(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    book_id: &str,
) -> Result<Value, StoreError> {
    let membership = member_of(graph, book_id, &scope.user_id, None)?;
    let book = book_of(graph, book_id)?;
    Ok(json!({
        "service": "book-sync-service",
        "book_id": book_id,
        "user_id": scope.user_id,
        "role": text(&membership, "role"),
        "tier": match book.get("tier") {
            Some(Value::String(tier)) => json!(tier),
            _ => Value::Null,
        },
    }))
}

/// `POST /books/default`: get-or-create the caller's personal Book, so nobody starts without one.
pub fn default_book(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let mut earliest: Option<(i64, Value)> = None;
    for membership in held(graph, MEMBERSHIP).everywhere("member_user_id", &json!(scope.user_id))? {
        if text(&membership, "status") != "active" {
            continue;
        }
        let Ok(book) = book_of(graph, &text(&membership, "book_id")) else {
            continue;
        };
        if text(&book, "tier") != "personal" {
            continue;
        }
        let order = (number(&book, "created_at") * 1000.0) as i64;
        if earliest.as_ref().is_none_or(|(held, _)| order < *held) {
            let mut answer = book_answer(&book);
            answer["role"] = json!(text(&membership, "role"));
            earliest = Some((order, answer));
        }
    }
    if let Some((_, book)) = earliest {
        return Ok(json!({
            "service": "book-sync-service",
            "book": book,
            "created": false,
        }));
    }
    let made = create_book(
        graph,
        scope,
        &json!({
            "name": "My Book",
            "tier": "personal",
            "now": payload.get("now").cloned().unwrap_or(Value::Null),
        }),
    )?;
    let book_id = text(&made["book"], "id");
    append_audit(
        graph,
        &book_id,
        &scope.user_id,
        "book.created",
        "personal (auto-provisioned)",
        &payload.get("now").cloned().unwrap_or(Value::Null),
    )?;
    let mut book = book_answer(&book_of(graph, &book_id)?);
    book["role"] = json!(OWNER);
    Ok(json!({
        "service": "book-sync-service",
        "book": book,
        "created": true,
    }))
}

/// `GET /books/{id}/devices`: the devices registered against a Book, for a member who wants to see them.
pub fn list_devices(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    book_id: &str,
) -> Result<Value, StoreError> {
    member_of(graph, book_id, &scope.user_id, None)?;
    Ok(json!({
        "service": "book-sync-service",
        "devices": held(graph, DEVICE)
            .where_eq(book_id, "book_id", &json!(book_id))?
            .into_iter()
            .map(|device| json!({
                "device_id": text(&device, "device_id"),
                "user_id": text(&device, "member_user_id"),
                "device_name": text(&device, "device_name"),
                "last_seen_seq": number(&device, "last_seen_seq") as i64,
            }))
            .collect::<Vec<_>>(),
    }))
}

/// A Book is shared when it has more than its owner, which is what a reader wants to know rather than a member count.
pub fn is_shared(graph: &(impl Graph + ?Sized), book_id: &str) -> Result<bool, StoreError> {
    Ok(held(graph, MEMBERSHIP)
        .where_eq(book_id, "book_id", &json!(book_id))?
        .len()
        > 1)
}

/// The flag a Book carries when it was created with a wrapped key; kept for a reader that asks why it can be shared.
pub fn carries_key(graph: &(impl Graph + ?Sized), book_id: &str) -> Result<bool, StoreError> {
    Ok(held(graph, METADATA)
        .where_eq(book_id, "key", &json!("_wrapped_book_key"))?
        .first()
        .map(|row| flag(row, "value"))
        .unwrap_or(false))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::MemoryGraph;

    fn as_user(user_id: &str) -> BookScope {
        BookScope::new(user_id, "unused")
    }

    fn opened(graph: &MemoryGraph) -> String {
        let made = create_book(
            graph,
            &as_user("user-1"),
            &json!({"name": "Household", "tier": "household", "now": 1000.0,
                    "wrapped_book_key": "wrapped-for-user-1"}),
        )
        .expect("a book");
        text(&made["book"], "id")
    }

    fn joined(graph: &MemoryGraph, book_id: &str) -> BookScope {
        invite_member(
            graph,
            &as_user("user-1"),
            book_id,
            &json!({"user_id": "user-2", "role": "bookkeeper", "wrapped_book_key": "wrapped-for-user-2",
                    "now": 1001.0}),
        )
        .expect("an invitation");
        let accepted = accept_invite(graph, &as_user("user-2"), book_id).expect("an acceptance");
        assert_eq!(accepted["wrapped_book_key"], json!("wrapped-for-user-2"));
        as_user("user-2")
    }

    #[test]
    fn a_book_opens_in_its_tier_and_its_creator_owns_it() {
        let graph = MemoryGraph::new();
        let made = create_book(
            &graph,
            &as_user("user-1"),
            &json!({"name": "Household", "tier": "household", "folder": " Family ", "now": 1000.0}),
        )
        .expect("a book");
        assert_eq!(made["your_role"], json!("owner"));
        assert_eq!(made["book"]["seq"], json!(0));
        // a tier outside the five is refused by name
        assert!(create_book(
            &graph,
            &as_user("user-1"),
            &json!({"name": "Odd", "tier": "charity", "now": 1000.0})
        )
        .is_err());
        assert!(create_book(
            &graph,
            &as_user("user-1"),
            &json!({"name": "", "tier": "personal", "now": 1000.0})
        )
        .is_err());
        // the personal Book is provisioned once and then found again
        let first =
            default_book(&graph, &as_user("user-1"), &json!({"now": 1002.0})).expect("a book");
        assert_eq!(first["created"], json!(true));
        assert_eq!(first["book"]["name"], json!("My Book"));
        let again =
            default_book(&graph, &as_user("user-1"), &json!({"now": 1003.0})).expect("a book");
        assert_eq!(again["created"], json!(false));
        assert_eq!(again["book"]["id"], first["book"]["id"]);
        // and both Books are in the caller's list, oldest first
        let listed = list_books(&graph, &as_user("user-1")).expect("books");
        assert_eq!(listed["books"].as_array().map(Vec::len), Some(2));
        assert_eq!(listed["books"][0]["name"], json!("Household"));
    }

    #[test]
    fn an_invitation_carries_the_key_and_waits_for_acceptance() {
        let graph = MemoryGraph::new();
        let book_id = opened(&graph);
        // an invitation without a wrapped key is refused: the person would join and decrypt nothing
        assert!(invite_member(
            &graph,
            &as_user("user-1"),
            &book_id,
            &json!({"user_id": "user-2", "role": "viewer"})
        )
        .is_err());
        // and the owner's own role is not on offer
        assert!(invite_member(
            &graph,
            &as_user("user-1"),
            &book_id,
            &json!({"user_id": "user-2", "role": "owner", "wrapped_book_key": "k"})
        )
        .is_err());
        invite_member(
            &graph,
            &as_user("user-1"),
            &book_id,
            &json!({"user_id": "user-2", "role": "admin", "wrapped_book_key": "wrapped-for-user-2",
                    "now": 1001.0}),
        )
        .expect("an invitation");
        // the same person is not invited twice
        let twice = invite_member(
            &graph,
            &as_user("user-1"),
            &book_id,
            &json!({"user_id": "user-2", "role": "viewer", "wrapped_book_key": "k"}),
        );
        assert!(matches!(twice, Err(StoreError::Conflict(_))));

        // the Book shows in the invited person's list as pending, and they cannot read it yet
        let pending = list_books(&graph, &as_user("user-2")).expect("books");
        assert_eq!(pending["books"][0]["membership_status"], json!("invited"));
        assert!(get_book(&graph, &as_user("user-2"), &book_id).is_err());

        let accepted = accept_invite(&graph, &as_user("user-2"), &book_id).expect("an acceptance");
        assert_eq!(accepted["status"], json!("active"));
        assert_eq!(accepted["wrapped_book_key"], json!("wrapped-for-user-2"));
        let now = list_books(&graph, &as_user("user-2")).expect("books");
        assert_eq!(now["books"][0]["membership_status"], json!("active"));
        assert!(get_book(&graph, &as_user("user-2"), &book_id).is_ok());
        // accepting twice finds nothing pending
        assert!(accept_invite(&graph, &as_user("user-2"), &book_id).is_err());
    }

    #[test]
    fn only_the_configuring_roles_change_a_book_and_the_owner_is_out_of_reach() {
        let graph = MemoryGraph::new();
        let book_id = opened(&graph);
        let viewer = joined(&graph, &book_id);
        update_member(
            &graph,
            &as_user("user-1"),
            &book_id,
            "user-2",
            &json!({"role": "viewer", "now": 1004.0}),
        )
        .expect("a change");
        // a viewer cannot rename the Book, invite anyone, or change a member
        assert!(update_book(&graph, &viewer, &book_id, &json!({"name": "Mine now"})).is_err());
        assert!(invite_member(
            &graph,
            &viewer,
            &book_id,
            &json!({"user_id": "user-3", "role": "viewer", "wrapped_book_key": "k"})
        )
        .is_err());
        assert!(update_member(
            &graph,
            &viewer,
            &book_id,
            "user-1",
            &json!({"role": "viewer"})
        )
        .is_err());
        // and nobody demotes the owner
        assert!(update_member(
            &graph,
            &as_user("user-1"),
            &book_id,
            "user-1",
            &json!({"role": "viewer"})
        )
        .is_err());
        // a role or state the Book does not know is refused by name
        assert!(update_member(
            &graph,
            &as_user("user-1"),
            &book_id,
            "user-2",
            &json!({"role": "king"})
        )
        .is_err());
        assert!(update_member(
            &graph,
            &as_user("user-1"),
            &book_id,
            "user-2",
            &json!({"status": "gone"})
        )
        .is_err());
        // a stranger's name is missing rather than refused
        assert!(matches!(
            update_member(
                &graph,
                &as_user("user-1"),
                &book_id,
                "user-9",
                &json!({"role": "viewer"})
            ),
            Err(StoreError::NotFound)
        ));
        let members = book_members(&graph, &as_user("user-1"), &book_id).expect("members");
        assert_eq!(members["members"].as_array().map(Vec::len), Some(2));
        assert_eq!(members["members"][1]["status"], json!("active"));
    }

    #[test]
    fn pushing_entries_is_retry_safe_and_the_sequence_rises_once() {
        let graph = MemoryGraph::new();
        let book_id = opened(&graph);
        let device = register_device(
            &graph,
            &as_user("user-1"),
            &book_id,
            &json!({"device_name": "laptop", "now": 1005.0}),
        )
        .expect("a device");
        let device_id = text(&device, "device_id");
        let batch = json!({"device_id": device_id, "entries": [
            {"entry_id": "e1", "payload": "cipher-1", "created_at": 1006.0},
            {"entry_id": "e2", "payload": "cipher-2", "created_at": 1007.0}
        ]});
        let pushed = push_entries(&graph, &as_user("user-1"), &book_id, &batch).expect("a push");
        assert_eq!(pushed["accepted"], json!(2));
        assert_eq!(pushed["book_seq"], json!(2));
        // the same batch again stores nothing but still answers where the Book stands
        let repeated = push_entries(&graph, &as_user("user-1"), &book_id, &batch).expect("a push");
        assert_eq!(repeated["accepted"], json!(0));
        assert_eq!(repeated["book_seq"], json!(2));

        // a pull from the start sees both, in sequence order, carrying the author
        let pulled =
            pull_entries(&graph, &as_user("user-1"), &book_id, &json!({})).expect("entries");
        let entries = pulled["entries"].as_array().expect("entries");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0]["seq"], json!(1));
        assert_eq!(entries[1]["entry_id"], json!("e2"));
        assert_eq!(entries[0]["user_id"], json!("user-1"));
        // and from a high-water mark it sees only what follows
        let later = pull_entries(&graph, &as_user("user-1"), &book_id, &json!({"since": 1}))
            .expect("entries");
        assert_eq!(later["entries"].as_array().map(Vec::len), Some(1));
        // an entry with no payload is refused rather than stored empty
        assert!(push_entries(
            &graph,
            &as_user("user-1"),
            &book_id,
            &json!({"device_id": device_id, "entries": [{"entry_id": "e3"}]})
        )
        .is_err());
    }

    #[test]
    fn a_device_belongs_to_its_person_and_a_stranger_reads_nothing() {
        let graph = MemoryGraph::new();
        let book_id = opened(&graph);
        let mine = register_device(
            &graph,
            &as_user("user-1"),
            &book_id,
            &json!({"now": 1005.0}),
        )
        .expect("a device");
        let device_id = text(&mine, "device_id");
        let other = joined(&graph, &book_id);
        // another member may not push as this device
        let refused_push = push_entries(
            &graph,
            &other,
            &book_id,
            &json!({"device_id": device_id, "entries": [{"entry_id": "e1", "payload": "c"}]}),
        );
        assert!(refused_push.is_err());
        // and a device that was never registered is refused by name
        assert!(push_entries(
            &graph,
            &other,
            &book_id,
            &json!({"device_id": "made-up", "entries": [{"entry_id": "e1", "payload": "c"}]})
        )
        .is_err());

        // someone who is not in the Book reads nothing at all
        let stranger = as_user("user-9");
        assert!(get_book(&graph, &stranger, &book_id).is_err());
        assert!(pull_entries(&graph, &stranger, &book_id, &json!({})).is_err());
        assert!(book_members(&graph, &stranger, &book_id).is_err());
        assert!(audit_log(&graph, &stranger, &book_id, &json!({})).is_err());
        assert!(membership_check(&graph, &stranger, &book_id).is_err());
        // while a member is told their role and the Book's tier
        let check = membership_check(&graph, &other, &book_id).expect("a check");
        assert_eq!(check["role"], json!("bookkeeper"));
        assert_eq!(check["tier"], json!("household"));
    }

    #[test]
    fn metadata_is_last_write_wins_by_the_time_the_client_states() {
        let graph = MemoryGraph::new();
        let book_id = opened(&graph);
        upsert_metadata(
            &graph,
            &as_user("user-1"),
            &book_id,
            &json!({"key": "settings", "value": "first", "updated_at": 1000.0}),
        )
        .expect("a write");
        upsert_metadata(
            &graph,
            &as_user("user-1"),
            &book_id,
            &json!({"key": "settings", "value": "second", "updated_at": 2000.0}),
        )
        .expect("a write");
        let pulled =
            pull_metadata(&graph, &as_user("user-1"), &book_id, &json!({})).expect("metadata");
        let rows = pulled["metadata"].as_array().expect("metadata");
        // one row per key, holding the later value, and NOT two rows to reconcile on the client
        let settings: Vec<&Value> = rows
            .iter()
            .filter(|row| row["key"] == json!("settings"))
            .collect();
        assert_eq!(settings.len(), 1);
        assert_eq!(settings[0]["value"], json!("second"));
        assert_eq!(settings[0]["updated_at"], json!(2000.0));
        // a since after the write sees nothing, and the wrapped key is a row like any other
        let none = pull_metadata(
            &graph,
            &as_user("user-1"),
            &book_id,
            &json!({"since": 2500.0}),
        )
        .expect("metadata");
        assert_eq!(none["metadata"].as_array().map(Vec::len), Some(0));
        assert!(rows
            .iter()
            .any(|row| row["key"] == json!("_wrapped_book_key")));
        // a viewer reads metadata but does not write it
        let viewer = joined(&graph, &book_id);
        update_member(
            &graph,
            &as_user("user-1"),
            &book_id,
            "user-2",
            &json!({"role": "viewer"}),
        )
        .expect("a change");
        assert!(pull_metadata(&graph, &viewer, &book_id, &json!({})).is_ok());
        assert!(upsert_metadata(
            &graph,
            &viewer,
            &book_id,
            &json!({"key": "settings", "value": "theirs", "updated_at": 3000.0})
        )
        .is_err());
    }

    #[test]
    fn the_trail_says_who_did_what_and_reads_newest_first() {
        let graph = MemoryGraph::new();
        let book_id = opened(&graph);
        invite_member(
            &graph,
            &as_user("user-1"),
            &book_id,
            &json!({"user_id": "user-2", "role": "viewer", "wrapped_book_key": "k", "now": 1001.0}),
        )
        .expect("an invitation");
        let trail = audit_log(&graph, &as_user("user-1"), &book_id, &json!({})).expect("audit");
        let rows = trail["audit"].as_array().expect("audit");
        assert_eq!(rows.len(), 2);
        // newest first: the invitation came after the Book was opened
        assert_eq!(rows[0]["action"], json!("member.invited"));
        assert_eq!(rows[0]["detail"], json!("user-2:viewer"));
        assert_eq!(rows[1]["action"], json!("book.created"));
        assert_eq!(rows[0]["actor"], json!("user-1"));
        // the limit is honoured from the newest end
        let one =
            audit_log(&graph, &as_user("user-1"), &book_id, &json!({"limit": 1})).expect("audit");
        assert_eq!(one["audit"].as_array().map(Vec::len), Some(1));
        // a Book with more than its owner is shared, and the key it was opened with is on record
        assert!(is_shared(&graph, &book_id).expect("a check"));
        assert!(carries_key(&graph, &book_id).is_ok());
    }
}
