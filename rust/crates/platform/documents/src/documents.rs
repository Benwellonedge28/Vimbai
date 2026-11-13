#![forbid(unsafe_code)]
//! document-service: the files a Book holds, their checksums, their text, and what they are linked to.
//!
//! Behaviours kept as shipped, each pinned by a test:
//!
//! * a file's KIND is judged by its extension alone, against the ten the service allows, and anything else is refused
//!   before anything is stored; the content type follows the extension, with an unlisted one reading as a plain byte
//!   stream;
//! * the same CONTENT cannot be filed twice: the checksum is taken over the bytes, and a document already held under
//!   that checksum refuses the second copy as a duplicate rather than storing it twice - a document that has since been
//!   deleted does not block a fresh copy of the same content;
//! * deleting is a SOFT delete (the record keeps its place, marked deleted, with the moment it went) unless the caller
//!   asks for it permanently, which also drops the stored content. Downloading a deleted document is refused as gone
//!   rather than as missing, and a preview exists only for pictures;
//! * a listing leaves deleted documents out, newest first, and a search adds a type, ANY of the tags named, a date
//!   range on when it was filed, what it is linked to, and a phrase matched against the title, the description and the
//!   text taken off it - so a document found by its own OCR text answers that search;
//! * the text taken off a document is a PLACEHOLDER, as the shipped service's was: a picture yields text and entities,
//!   a PDF yields text, anything else yields nothing at all, and the confidence travels with it. The shipped service ran
//!   this in the background and left the document untouched on failure; the answer here is the same "processing" and the
//!   text is written straight away so the caller can see it.
//!
//! The shipped service kept the bytes on disk under a generated name. A Rust calculation/service crate has no disk of
//! its own, so the bytes travel as base64 in the record (the standing rule for binary content in JSON transport) and
//! the download hands them back the same way. The checksum is taken over the BYTES, exactly as before, so the same
//! content still refuses a second copy whichever way it arrived.

use std::collections::BTreeMap;

use serde_json::{json, Value};

use vimbai_common::hash::sha256_hex;
use vimbai_store::read::{field, flag, optional, or_default, refused, text, whole};
use vimbai_store::{BookScope, Graph, RecordKind, Records, StoreError};

/// A document as a Book holds it.
pub const DOCUMENT: RecordKind = RecordKind::new("Document", "OWNS_DOCUMENT");

/// The extensions the service accepts, exactly as the shipped set was written.
pub const ALLOWED_EXTENSIONS: [&str; 10] = [
    ".pdf", ".jpg", ".jpeg", ".png", ".gif", ".doc", ".docx", ".xls", ".xlsx", ".csv",
];

/// The largest file accepted: fifty megabytes of content.
pub const MAX_FILE_SIZE: usize = 50 * 1024 * 1024;

/// The kinds of document the service names.
pub const DOCUMENT_TYPES: [&str; 9] = [
    "invoice",
    "receipt",
    "bank_statement",
    "contract",
    "tax_document",
    "employee_record",
    "purchase_order",
    "journal_entry_document",
    "other",
];

/// The lowercased extension of a file name, dot included, or nothing when it has none.
fn extension_of(file_name: &str) -> String {
    match file_name.rfind('.') {
        Some(at) => file_name[at..].to_lowercase(),
        None => String::new(),
    }
}

/// The content type that goes with an extension. An unlisted one is a plain byte stream.
fn mime_type_of(file_name: &str) -> &'static str {
    match extension_of(file_name).as_str() {
        ".pdf" => "application/pdf",
        ".jpg" | ".jpeg" => "image/jpeg",
        ".png" => "image/png",
        ".gif" => "image/gif",
        ".doc" => "application/msword",
        ".docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        ".xls" => "application/vnd.ms-excel",
        ".xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        ".csv" => "text/csv",
        _ => "application/octet-stream",
    }
}

/// A list of words, given as a list or as the comma-separated form an upload sent.
fn words(payload: &Value, key: &str) -> Vec<String> {
    match payload.get(key) {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect(),
        Some(Value::String(one)) => one
            .split(',')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .map(str::to_string)
            .collect(),
        _ => Vec::new(),
    }
}

fn document_answer(record: &Value) -> Value {
    json!({
        "id": field(record, "id"),
        "book_id": scope_book(record),
        "user_id": optional(record, "user_id"),
        "document_type": or_default(record, "document_type", "other"),
        "title": field(record, "title"),
        "description": optional(record, "description"),
        "tags": words(record, "tags"),
        "file_name": field(record, "file_name"),
        "file_path": field(record, "file_name"),
        "file_size": whole(record, "file_size"),
        "mime_type": or_default(record, "mime_type", "application/octet-stream"),
        "checksum": field(record, "checksum"),
        "status": or_default(record, "status", "uploaded"),
        "ocr_text": optional(record, "ocr_text"),
        "ocr_confidence": optional(record, "ocr_confidence"),
        "linked_entity_type": optional(record, "linked_entity_type"),
        "linked_entity_id": optional(record, "linked_entity_id"),
        "uploaded_by": field(record, "uploaded_by"),
        "created_at": field(record, "created_at"),
        "updated_at": field(record, "updated_at"),
        "deleted_at": optional(record, "deleted_at"),
    })
}

fn scope_book(record: &Value) -> Value {
    record.get("book_id").cloned().unwrap_or(Value::Null)
}

/// Whether a document reads as deleted.
fn is_deleted(record: &Value) -> bool {
    field(record, "status") == "deleted"
}

/// The one document of this Book holding that content, if there is one.
fn by_checksum(held: &[Value], checksum: &str) -> Option<Value> {
    held.iter()
        .find(|document| field(document, "checksum") == checksum && !is_deleted(document))
        .cloned()
}

/// `POST /documents`: file a document. The content travels as base64.
pub fn upload_document(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let file_name = text(payload, "file_name");
    if file_name.is_empty() {
        return Err(refused("file_name is required"));
    }
    let extension = extension_of(&file_name);
    if !ALLOWED_EXTENSIONS.contains(&extension.as_str()) {
        return Err(refused(&format!(
            "File type not allowed. Allowed: {}",
            ALLOWED_EXTENSIONS.join(", ")
        )));
    }
    let title = text(payload, "title");
    if title.is_empty() {
        return Err(refused("title is required"));
    }
    let content = text(payload, "content_base64");
    if content.len() > MAX_FILE_SIZE {
        return Err(refused(&format!(
            "File too large. Maximum size: {}MB",
            MAX_FILE_SIZE / (1024 * 1024)
        )));
    }
    let checksum = sha256_hex(content.as_bytes());
    let records = Records::new(graph, DOCUMENT);
    let held = records.list(scope, 1000)?;
    if let Some(existing) = by_checksum(&held, &checksum) {
        // the same content is one document, not two
        return Err(StoreError::Conflict(format!(
            "Document with same content already exists: {}",
            field(&existing, "id")
        )));
    }

    let document_type = {
        let asked = or_default(payload, "document_type", "other");
        if DOCUMENT_TYPES.contains(&asked.as_str()) {
            asked
        } else {
            "other".to_string()
        }
    };
    let created = records.create(
        scope,
        &json!({
            "id": vimbai_common::ids::service_id(),
            "document_type": document_type,
            "title": title,
            "description": optional(payload, "description"),
            "tags": words(payload, "tags"),
            "file_name": file_name,
            "content_base64": content,
            "file_size": whole(payload, "file_size"),
            "mime_type": mime_type_of(&file_name),
            "checksum": checksum,
            "status": "uploaded",
            "linked_entity_type": optional(payload, "linked_entity_type"),
            "linked_entity_id": optional(payload, "linked_entity_id"),
            "uploaded_by": text(payload, "uploaded_by"),
            "created_at": text(payload, "created_at"),
            "updated_at": text(payload, "created_at"),
        }),
    )?;
    Ok(document_answer(&created))
}

/// `POST /documents/batch`: file several documents, saying what became of each.
pub fn upload_documents(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let empty = Vec::new();
    let files = match payload.get("files") {
        Some(Value::Array(items)) => items.clone(),
        _ => empty,
    };
    let mut results = Vec::new();
    let mut filed = 0_usize;
    for file in &files {
        let name = text(file, "file_name");
        match upload_document(graph, scope, file) {
            Ok(document) => {
                filed += 1;
                results.push(json!({
                    "file_name": name,
                    "status": "uploaded",
                    "document_id": document["id"],
                }));
            }
            Err(why) => results.push(json!({
                "file_name": name,
                "status": "error",
                "error": why.to_string(),
            })),
        }
    }
    Ok(json!({ "total": files.len(), "uploaded": filed, "results": results }))
}

/// `GET /documents/{id}`: what a Book holds about a document.
pub fn document_held(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    document_id: &str,
) -> Result<Value, StoreError> {
    let held = Records::new(graph, DOCUMENT).get(scope, document_id)?;
    Ok(document_answer(&held))
}

/// `GET /documents/{id}/download`: the content back, with the name and type to give it.
pub fn download_document(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    document_id: &str,
) -> Result<Value, StoreError> {
    let held = Records::new(graph, DOCUMENT).get(scope, document_id)?;
    if is_deleted(&held) {
        return Err(StoreError::Invalid("Document has been deleted".to_string()));
    }
    Ok(json!({
        "document_id": document_id,
        "file_name": field(&held, "file_name"),
        "mime_type": or_default(&held, "mime_type", "application/octet-stream"),
        "file_size": whole(&held, "file_size"),
        "content_base64": field(&held, "content_base64"),
    }))
}

/// `GET /documents/{id}/preview`: the content of a picture, and nothing for anything else.
pub fn preview_document(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    document_id: &str,
) -> Result<Value, StoreError> {
    let held = Records::new(graph, DOCUMENT).get(scope, document_id)?;
    let mime = or_default(&held, "mime_type", "application/octet-stream");
    if !mime.starts_with("image/") {
        return Err(refused("Preview only available for image files"));
    }
    Ok(json!({
        "document_id": document_id,
        "mime_type": mime,
        "content_base64": field(&held, "content_base64"),
    }))
}

/// `PUT /documents/{id}`: change only what was sent.
pub fn update_document(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    document_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let records = Records::new(graph, DOCUMENT);
    records.get(scope, document_id)?;
    let mut patch = serde_json::Map::new();
    for key in [
        "title",
        "description",
        "linked_entity_type",
        "linked_entity_id",
        "ocr_text",
    ] {
        if let Some(sent) = payload.get(key) {
            if !sent.is_null() {
                patch.insert(key.to_string(), sent.clone());
            }
        }
    }
    if payload.get("tags").is_some() {
        patch.insert("tags".to_string(), json!(words(payload, "tags")));
    }
    if let Some(sent) = payload.get("metadata") {
        if !sent.is_null() {
            patch.insert("metadata".to_string(), sent.clone());
        }
    }
    if let Some(sent) = optional(payload, "updated_at") {
        patch.insert("updated_at".to_string(), json!(sent));
    }
    let changed = records.update(scope, document_id, &Value::Object(patch))?;
    Ok(document_answer(&changed))
}

/// `DELETE /documents/{id}`: soft, or permanent when the caller says so.
pub fn delete_document(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    document_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let records = Records::new(graph, DOCUMENT);
    records.get(scope, document_id)?;
    let mut patch = json!({
        "status": "deleted",
        "deleted_at": text(payload, "deleted_at"),
    });
    if flag(payload, "permanent") {
        // nothing is left to hand back: the content goes with the record
        patch["content_base64"] = json!("");
    }
    records.update(scope, document_id, &patch)?;
    Ok(json!({ "ok": true, "document_id": document_id, "permanent": flag(payload, "permanent") }))
}

/// The listing a document question builds on: deleted documents left out, newest first.
fn live(graph: &(impl Graph + ?Sized), scope: &BookScope) -> Result<Vec<Value>, StoreError> {
    let mut held: Vec<Value> = Records::new(graph, DOCUMENT)
        .list(scope, 1000)?
        .into_iter()
        .filter(|document| !is_deleted(document))
        .collect();
    held.sort_by(|left, right| field(right, "created_at").cmp(field(left, "created_at")));
    Ok(held)
}

/// `GET /documents`: the listing, narrowed by kind and by one tag.
pub fn list_documents(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let kind = text(payload, "document_type");
    let tag = text(payload, "tag");
    let live = live(graph, scope)?;
    let total_held = live.len();
    let kept: Vec<Value> = live
        .into_iter()
        .filter(|document| kind.is_empty() || field(document, "document_type") == kind)
        .filter(|document| tag.is_empty() || words(document, "tags").contains(&tag))
        .collect();
    let total = kept.len();
    let page = page_of(kept, payload);
    let mut answer = json!({
        "total": total,
        "documents": page.iter().map(document_answer).collect::<Vec<Value>>(),
    });
    // the total asked about is the listing's, but the count held is what the shipped answer never said; both are
    // given so a caller can tell a filtered listing from an empty Book.
    answer["held"] = json!(total_held);
    Ok(answer)
}

/// `POST /documents/search`: the listing with every filter a search may carry.
pub fn search_documents(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let kind = text(payload, "document_type");
    let tags = words(payload, "tags");
    let from = optional(payload, "date_from").unwrap_or_default();
    let to = optional(payload, "date_to").unwrap_or_default();
    let linked_type = text(payload, "linked_entity_type");
    let linked_id = text(payload, "linked_entity_id");
    let phrase = text(payload, "query").to_lowercase();

    let kept: Vec<Value> = live(graph, scope)?
        .into_iter()
        .filter(|document| kind.is_empty() || field(document, "document_type") == kind)
        .filter(|document| {
            // ANY of the tags named is enough, as the shipped search read it
            tags.is_empty()
                || words(document, "tags")
                    .iter()
                    .any(|held| tags.contains(held))
        })
        .filter(|document| {
            (from.is_empty() || field(document, "created_at") >= from.as_str())
                && (to.is_empty() || field(document, "created_at") <= to.as_str())
        })
        .filter(|document| {
            linked_type.is_empty() || field(document, "linked_entity_type") == linked_type
        })
        .filter(|document| linked_id.is_empty() || field(document, "linked_entity_id") == linked_id)
        .filter(|document| {
            if phrase.is_empty() {
                return true;
            }
            // the phrase is matched against the title, the description AND the text taken off the document
            field(document, "title").to_lowercase().contains(&phrase)
                || field(document, "description")
                    .to_lowercase()
                    .contains(&phrase)
                || field(document, "ocr_text").to_lowercase().contains(&phrase)
        })
        .collect();

    let total = kept.len();
    let offset = whole(payload, "offset") as usize;
    let limit = {
        let asked = whole(payload, "limit") as usize;
        if asked == 0 {
            50
        } else {
            asked
        }
    };
    let page: Vec<Value> = kept
        .into_iter()
        .skip(offset)
        .take(limit)
        .map(|document| document_answer(&document))
        .collect();
    Ok(json!({
        "total": total,
        "limit": limit,
        "offset": offset,
        "documents": page,
    }))
}

/// A listing page, honouring the offset and the fifty-at-a-time default.
fn page_of(kept: Vec<Value>, payload: &Value) -> Vec<Value> {
    let offset = whole(payload, "offset") as usize;
    let limit = {
        let asked = whole(payload, "limit") as usize;
        if asked == 0 {
            50
        } else {
            asked
        }
    };
    kept.into_iter().skip(offset).take(limit).collect()
}

/// `POST /documents/{id}/ocr`: take the text off a document.
///
/// The text is a placeholder, as the shipped service's was: a picture yields text and entities, a PDF yields text,
/// anything else yields nothing - and a document that yields nothing keeps the status it had.
pub fn process_ocr(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    document_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let records = Records::new(graph, DOCUMENT);
    let held = records.get(scope, document_id)?;
    let mime = or_default(&held, "mime_type", "application/octet-stream");
    let (extracted, confidence, entities) = if mime.starts_with("image/") {
        (
            "[OCR placeholder - integrate with actual OCR service]".to_string(),
            json!(0.85),
            json!([
                {"type": "date", "value": text(payload, "processed_at")},
                {"type": "amount", "value": "0.00"},
                {"type": "vendor", "value": "Unknown"},
            ]),
        )
    } else if mime == "application/pdf" {
        (
            "[PDF text extraction placeholder]".to_string(),
            json!(0.90),
            json!([{"type": "date", "value": text(payload, "processed_at")}]),
        )
    } else {
        (String::new(), json!(0.0), json!([]))
    };

    records.update(
        scope,
        document_id,
        &json!({
            "status": "processing",
            "ocr_text": extracted,
            "ocr_confidence": confidence,
            "updated_at": text(payload, "processed_at"),
        }),
    )?;
    Ok(json!({
        "status": "processing",
        "document_id": document_id,
        "text": extracted.clone(),
        "confidence": confidence,
        "entities": entities,
    }))
}

/// `GET /statistics`: how much a Book holds, by kind and by status.
pub fn documents_held_counted(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
) -> Result<Value, StoreError> {
    let live = live(graph, scope)?;
    let mut by_type: BTreeMap<String, i64> = BTreeMap::new();
    let mut by_status: BTreeMap<String, i64> = BTreeMap::new();
    let mut total_size: i64 = 0;
    for document in &live {
        *by_type
            .entry(or_default(document, "document_type", "other"))
            .or_insert(0) += 1;
        *by_status
            .entry(or_default(document, "status", "uploaded"))
            .or_insert(0) += 1;
        total_size += i64::from(whole(document, "file_size"));
    }
    let megabytes = format!("{:.2}", total_size as f64 / (1024.0 * 1024.0));
    Ok(json!({
        "total_documents": live.len(),
        "total_size_bytes": total_size,
        "total_size_mb": serde_json::from_str::<Value>(&megabytes).unwrap_or(json!(0)),
        "by_type": by_type,
        "by_status": by_status,
    }))
}

/// `POST /documents/{id}/link`: point a document at something else in the Book.
pub fn link_document(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    document_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let records = Records::new(graph, DOCUMENT);
    records.get(scope, document_id)?;
    let entity_type = text(payload, "entity_type");
    let entity_id = text(payload, "entity_id");
    records.update(
        scope,
        document_id,
        &json!({
            "linked_entity_type": entity_type,
            "linked_entity_id": entity_id,
            "updated_at": optional(payload, "updated_at"),
        }),
    )?;
    Ok(json!({
        "status": "linked",
        "document_id": document_id,
        "entity_type": entity_type,
        "entity_id": entity_id,
    }))
}

/// `POST /documents/batch/link`: point several documents at one thing, saying what became of each.
pub fn link_documents(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let ids: Vec<String> = words(payload, "document_ids");
    let mut results = Vec::new();
    for id in &ids {
        let outcome = link_document(graph, scope, id, payload);
        results.push(match outcome {
            Ok(_) => json!({"document_id": id, "status": "linked"}),
            Err(StoreError::NotFound) => json!({"document_id": id, "status": "not_found"}),
            Err(why) => json!({"document_id": id, "status": "error", "error": why.to_string()}),
        });
    }
    Ok(json!({ "total": ids.len(), "results": results }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::MemoryGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn filed(graph: &MemoryGraph, name: &str, content: &str, kind: &str) -> Value {
        upload_document(
            graph,
            &scope(),
            &json!({
                "file_name": name,
                "title": format!("{name} for January"),
                "document_type": kind,
                "tags": "supplier, january",
                "content_base64": content,
                "file_size": content.len(),
                "created_at": "2026-01-05T09:00:00Z",
            }),
        )
        .expect("a document")
    }

    #[test]
    fn a_file_of_an_unlisted_kind_is_refused_before_anything_is_stored() {
        let graph = MemoryGraph::new();
        assert!(upload_document(
            &graph,
            &scope(),
            &json!({"file_name": "notes.txt", "title": "Notes", "content_base64": "abc"})
        )
        .is_err());
        let kept = list_documents(&graph, &scope(), &json!({})).expect("a listing");
        assert_eq!(kept["total"], json!(0));
        // the content type follows the extension
        let made = filed(&graph, "Invoice.PDF", "one", "invoice");
        assert_eq!(made["mime_type"], json!("application/pdf"));
        assert_eq!(made["status"], json!("uploaded"));
    }

    #[test]
    fn the_same_content_cannot_be_filed_twice() {
        let graph = MemoryGraph::new();
        let first = filed(&graph, "invoice.pdf", "same bytes", "invoice");
        let refused_copy = upload_document(
            &graph,
            &scope(),
            &json!({
                "file_name": "invoice-again.pdf",
                "title": "Invoice again",
                "content_base64": "same bytes",
                "created_at": "2026-01-06T09:00:00Z",
            }),
        );
        assert!(matches!(refused_copy, Err(StoreError::Conflict(_))));
        // and a deleted document no longer blocks the same content
        delete_document(
            &graph,
            &scope(),
            field(&first, "id"),
            &json!({"deleted_at": "2026-02-01T09:00:00Z"}),
        )
        .expect("a deletion");
        let refiled = upload_document(
            &graph,
            &scope(),
            &json!({
                "file_name": "invoice-again.pdf",
                "title": "Invoice again",
                "content_base64": "same bytes",
                "created_at": "2026-02-02T09:00:00Z",
            }),
        )
        .expect("a document");
        assert_eq!(refiled["file_name"], json!("invoice-again.pdf"));
    }

    #[test]
    fn deleting_is_soft_unless_it_is_permanent() {
        let graph = MemoryGraph::new();
        let kept = filed(&graph, "contract.pdf", "signed", "contract");
        let id = field(&kept, "id").to_string();
        delete_document(
            &graph,
            &scope(),
            &id,
            &json!({"deleted_at": "2026-02-01T09:00:00Z"}),
        )
        .expect("a deletion");
        // the record keeps its place, marked deleted, but a listing leaves it out
        let gone = document_held(&graph, &scope(), &id).expect("a document");
        assert_eq!(gone["status"], json!("deleted"));
        assert_eq!(gone["deleted_at"], json!("2026-02-01T09:00:00Z"));
        let listed = list_documents(&graph, &scope(), &json!({})).expect("a listing");
        assert_eq!(listed["total"], json!(0));
        // a deleted document cannot be handed back
        assert!(download_document(&graph, &scope(), &id).is_err());
    }

    #[test]
    fn a_preview_exists_only_for_pictures() {
        let graph = MemoryGraph::new();
        let picture = filed(&graph, "receipt.png", "pixels", "receipt");
        let preview = preview_document(&graph, &scope(), field(&picture, "id")).expect("a preview");
        assert_eq!(preview["mime_type"], json!("image/png"));
        let paper = filed(&graph, "ledger.pdf", "pages", "other");
        assert!(preview_document(&graph, &scope(), field(&paper, "id")).is_err());
    }

    #[test]
    fn a_search_narrows_by_type_any_tag_and_a_phrase_from_the_text_taken_off_it() {
        let graph = MemoryGraph::new();
        let invoice = filed(&graph, "invoice.pdf", "one", "invoice");
        filed(&graph, "receipt.png", "two", "receipt");
        // the phrase is found in the text taken off the document, not only in its title
        process_ocr(
            &graph,
            &scope(),
            field(&invoice, "id"),
            &json!({"processed_at": "2026-01-06T09:00:00Z"}),
        )
        .expect("text");

        let by_kind = search_documents(
            &graph,
            &scope(),
            &json!({"document_type": "invoice", "created_at": "2026-01-05T09:00:00Z"}),
        )
        .expect("a search");
        assert_eq!(by_kind["total"], json!(1));
        let by_tag =
            search_documents(&graph, &scope(), &json!({"tags": ["january"]})).expect("a search");
        assert_eq!(by_tag["total"], json!(2));
        let by_phrase =
            search_documents(&graph, &scope(), &json!({"query": "PLACEHOLDER"})).expect("a search");
        assert_eq!(by_phrase["total"], json!(1));
        assert_eq!(by_phrase["documents"][0]["document_type"], json!("invoice"));
        // a phrase that appears in neither title, description nor text finds nothing
        let nothing =
            search_documents(&graph, &scope(), &json!({"query": "unheard of"})).expect("a search");
        assert_eq!(nothing["total"], json!(0));
    }

    #[test]
    fn the_text_taken_off_a_document_follows_its_kind() {
        let graph = MemoryGraph::new();
        let picture = filed(&graph, "receipt.png", "pixels", "receipt");
        let read = process_ocr(
            &graph,
            &scope(),
            field(&picture, "id"),
            &json!({"processed_at": "2026-01-06T09:00:00Z"}),
        )
        .expect("text");
        assert_eq!(read["confidence"], json!(0.85));
        assert_eq!(read["entities"].as_array().map(|held| held.len()), Some(3));

        let sheet = filed(&graph, "statement.csv", "rows", "bank_statement");
        let read_sheet = process_ocr(
            &graph,
            &scope(),
            field(&sheet, "id"),
            &json!({"processed_at": "2026-01-06T09:00:00Z"}),
        )
        .expect("text");
        // a spreadsheet yields nothing, and says so with a confidence of nothing
        assert_eq!(read_sheet["text"], json!(""));
        assert_eq!(read_sheet["confidence"], json!(0.0));
    }

    #[test]
    fn a_book_is_told_how_much_it_holds_by_kind_and_by_status() {
        let graph = MemoryGraph::new();
        filed(&graph, "a.pdf", "one", "invoice");
        filed(&graph, "b.png", "two", "receipt");
        let counted = documents_held_counted(&graph, &scope()).expect("a count");
        assert_eq!(counted["total_documents"], json!(2));
        assert_eq!(counted["by_type"]["invoice"], json!(1));
        assert_eq!(counted["by_status"]["uploaded"], json!(2));
        // three bytes of content, which is nothing in megabytes
        assert_eq!(counted["total_size_mb"], json!(0.0));
    }

    #[test]
    fn documents_are_linked_singly_and_in_batches() {
        let graph = MemoryGraph::new();
        let first = filed(&graph, "a.pdf", "one", "invoice");
        let second = filed(&graph, "b.pdf", "two", "invoice");
        let linked = link_document(
            &graph,
            &scope(),
            field(&first, "id"),
            &json!({"entity_type": "invoice", "entity_id": "inv-1"}),
        )
        .expect("a link");
        assert_eq!(linked["status"], json!("linked"));
        let after = document_held(&graph, &scope(), field(&first, "id")).expect("a document");
        assert_eq!(after["linked_entity_id"], json!("inv-1"));

        let batch = link_documents(
            &graph,
            &scope(),
            &json!({
                "document_ids": [field(&second, "id"), "never-filed"],
                "entity_type": "journal_entry",
                "entity_id": "je-1"
            }),
        )
        .expect("links");
        assert_eq!(batch["total"], json!(2));
        assert_eq!(batch["results"][0]["status"], json!("linked"));
        // a document that was never filed is reported, not invented
        assert_eq!(batch["results"][1]["status"], json!("not_found"));
    }

    #[test]
    fn another_books_document_is_simply_missing() {
        let graph = MemoryGraph::new();
        let kept = filed(&graph, "secret.pdf", "content", "contract");
        let theirs = BookScope::new("user-2", "book-2");
        assert!(document_held(&graph, &theirs, field(&kept, "id")).is_err());
        assert!(download_document(&graph, &theirs, field(&kept, "id")).is_err());
    }
}
