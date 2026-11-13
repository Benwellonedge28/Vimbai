#![forbid(unsafe_code)]
//! multimodal-pipeline-service: the records a Book's inbox holds, and the corrections made to them.
//!
//! A capture is what the pipeline took in (a document, a recording, a photograph or typed text), what it read out of it,
//! and where that reading stands. The inbox is the same rows looked at by state: everything not yet accepted reads as
//! incomplete, an accepted record reads as organized, and a person correcting a reading marks the row corrected.
//!
//! The service's own rules, all kept:
//!
//! * an input is one of five kinds, and a reading is in one of seven states; anything else is refused by name rather than
//!   stored under a word nobody agreed on;
//! * the inbox view is one of three, and a fourth is refused as unprocessable - the shipped service answered 422 rather
//!   than 400, because the request was understood and its value was wrong;
//! * the inbox limit is held between one and five hundred. A caller asking for a thousand rows gets five hundred, and one
//!   asking for none gets one, rather than an empty answer that looks like an empty inbox;
//! * organizing is idempotent. Accepting a record the Book has already accepted answers the record unchanged instead of
//!   stamping a second finishing time over the first;
//! * a correction belongs to a capture, and correcting it marks the capture `user_corrected`. The correction is kept as
//!   its own record, with the field it touched and the value it started from, so what a person changed is readable later.
//!
//! ONE DELIBERATE DIFFERENCE, and the honest half of this port. The shipped service created the task and then handed it to
//! a background worker that read the document or recording and wrote the extracted figures back. Nothing in this crate
//! reads a document, hears a recording or calls a model: a task is recorded as `received` and stays there until something
//! else answers for it. The answer says so rather than leaving a caller to poll forever for a reading that is not coming.

use serde_json::{json, Value};

use vimbai_common::ids::service_id;
use vimbai_store::read::{number, text};
use vimbai_store::{BookScope, Graph, RecordKind, Records, StoreError};

/// A capture the pipeline took in.
pub const CAPTURE: RecordKind = RecordKind::new("MultimodalCapture", "OWNS_MULTIMODAL_CAPTURE");
/// A person's correction to what was read out of a capture.
pub const CORRECTION: RecordKind = RecordKind::new("CaptureCorrection", "OWNS_CAPTURE_CORRECTION");

/// The five kinds of input the pipeline takes.
pub const INPUT_KINDS: [&str; 5] = ["document", "audio", "image", "text", "video"];

/// The seven states a capture can be in.
pub const CAPTURE_STATES: [&str; 7] = [
    "received",
    "processing",
    "ai_extracted",
    "review_pending",
    "user_corrected",
    "completed",
    "failed",
];

/// Captures still being processed, awaiting a person, or failed.
const INCOMPLETE: [&str; 6] = [
    "received",
    "processing",
    "ai_extracted",
    "review_pending",
    "user_corrected",
    "failed",
];

/// The three views of the inbox.
const VIEWS: [&str; 3] = ["incomplete", "organized", "all"];

fn stamp(payload: &Value) -> Value {
    match payload.get("now") {
        Some(stated) if !stated.is_null() => stated.clone(),
        _ => json!(vimbai_common::clock::moment()),
    }
}

fn state_of(payload: &Value, fallback: &str) -> Result<String, StoreError> {
    let named = text(payload, "status");
    if named.is_empty() {
        return Ok(fallback.to_string());
    }
    if CAPTURE_STATES.contains(&named.as_str()) {
        return Ok(named);
    }
    Err(StoreError::Invalid(format!(
        "status must be one of: {} (got {named})",
        CAPTURE_STATES.join(", ")
    )))
}

/// `POST /tasks`: take in a capture.
pub fn create_capture(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let kind = text(payload, "input_type");
    if !INPUT_KINDS.contains(&kind.as_str()) {
        return Err(StoreError::Invalid(format!(
            "input_type must be one of: {} (got {kind})",
            INPUT_KINDS.join(", ")
        )));
    }
    let moment = stamp(payload);
    Records::new(graph, CAPTURE).create(
        scope,
        &json!({
            "id": service_id(),
            "input_type": kind,
            "input_url": payload.get("input_url").cloned().unwrap_or(Value::Null),
            "input_raw_text": text(payload, "input_raw_text"),
            "status": state_of(payload, "received")?,
            "processing_start_time": Value::Null,
            "processing_end_time": Value::Null,
            "last_review_request_time": Value::Null,
            "document_result": payload.get("document_result").cloned().unwrap_or(Value::Null),
            "audio_result": payload.get("audio_result").cloned().unwrap_or(Value::Null),
            "image_result": payload.get("image_result").cloned().unwrap_or(Value::Null),
            "suggested_journal_entry": payload.get("suggested_journal_entry").cloned().unwrap_or(Value::Null),
            "linked_vimbai_entity_id": text(payload, "linked_vimbai_entity_id"),
            "errors": payload.get("errors").cloned().unwrap_or_else(|| json!([])),
            "metadata": payload.get("metadata").cloned().unwrap_or_else(|| json!({})),
            "ai_model_version": text(payload, "ai_model_version"),
            "created_at": moment,
            "updated_at": moment,
            // nothing here reads a document or hears a recording: the capture waits for whatever does
            "reading_queued": false,
        }),
    )
}

/// `GET /tasks`: every capture this Book holds.
pub fn captures(graph: &(impl Graph + ?Sized), scope: &BookScope) -> Result<Value, StoreError> {
    Ok(Value::Array(Records::new(graph, CAPTURE).list(scope, 500)?))
}

/// `GET /inbox`: the same rows, looked at by state.
pub fn inbox(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let view = match text(payload, "view") {
        nothing if nothing.is_empty() => "incomplete".to_string(),
        named if VIEWS.contains(&named.as_str()) => named,
        other => {
            return Err(StoreError::Invalid(format!(
                "view must be one of: {} (got {other})",
                VIEWS.join(", ")
            )))
        }
    };
    // a caller asking for a thousand rows gets five hundred, and one asking for none gets one
    let asked = match payload.get("limit") {
        Some(_) => number(payload, "limit") as i64,
        None => 100,
    };
    let limit = asked.clamp(1, 500) as usize;

    let rows = Records::new(graph, CAPTURE).list(scope, 500)?;
    let kept: Vec<Value> = rows
        .into_iter()
        .filter(|row| match view.as_str() {
            "organized" => text(row, "status") == "completed",
            "all" => true,
            _ => INCOMPLETE.contains(&text(row, "status").as_str()),
        })
        .take(limit)
        .collect();
    Ok(Value::Array(kept))
}

/// `GET /inbox/summary`: what the inbox badge counts.
pub fn inbox_summary(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
) -> Result<Value, StoreError> {
    let rows = Records::new(graph, CAPTURE).list(scope, 500)?;
    let mut counts = serde_json::Map::new();
    for state in CAPTURE_STATES {
        counts.insert(state.to_string(), json!(0));
    }
    for row in &rows {
        let state = text(row, "status");
        let held = counts.get(&state).and_then(Value::as_i64).unwrap_or(0);
        counts.insert(state, json!(held + 1));
    }
    let incomplete: i64 = INCOMPLETE
        .iter()
        .map(|state| counts.get(*state).and_then(Value::as_i64).unwrap_or(0))
        .sum();
    let organized = counts.get("completed").and_then(Value::as_i64).unwrap_or(0);
    let mut answer = Value::Object(counts);
    answer["total_incomplete"] = json!(incomplete);
    answer["total_organized"] = json!(organized);
    Ok(answer)
}

/// `GET /tasks/{id}`: one capture.
pub fn capture(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    task_id: &str,
) -> Result<Value, StoreError> {
    Records::new(graph, CAPTURE).get(scope, task_id)
}

/// `POST /inbox/{id}/organize`: accept a capture into the Book.
///
/// Idempotent: accepting one that is already accepted answers it unchanged, rather than stamping a second finishing time
/// over the first.
pub fn organize_capture(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    task_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let held = capture(graph, scope, task_id)?;
    if text(&held, "status") == "completed" {
        return Ok(held);
    }
    Records::new(graph, CAPTURE).update(
        scope,
        task_id,
        &json!({
            "status": "completed",
            "processing_end_time": stamp(payload),
            "updated_at": stamp(payload),
        }),
    )
}

/// `PUT /tasks/{id}`: change the fields the caller named, and only those.
pub fn update_capture(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    task_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    capture(graph, scope, task_id)?;
    let mut change = json!({"updated_at": stamp(payload)});
    if payload.get("status").is_some() {
        change["status"] = json!(state_of(payload, "received")?);
    }
    for field in [
        "processing_start_time",
        "processing_end_time",
        "last_review_request_time",
        "document_result",
        "audio_result",
        "image_result",
        "suggested_journal_entry",
        "linked_vimbai_entity_id",
        "errors",
        "metadata",
        "ai_model_version",
        "input_url",
        "input_raw_text",
    ] {
        if payload.get(field).is_some() {
            change[field] = payload[field].clone();
        }
    }
    Records::new(graph, CAPTURE).update(scope, task_id, &change)
}

/// `DELETE /tasks/{id}`: forget a capture.
pub fn delete_capture(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    task_id: &str,
) -> Result<Value, StoreError> {
    capture(graph, scope, task_id)?;
    Records::new(graph, CAPTURE).delete(scope, task_id)?;
    Ok(json!({"ok": true}))
}

/// `POST /tasks/{id}/corrections`: record a person's correction to what was read.
///
/// The capture is marked `user_corrected`, because a reading a person has overruled is no longer what the pipeline
/// concluded. The correction keeps the field it touched and the value it started from, so what was changed stays readable
/// after the fact.
pub fn submit_correction(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    task_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    capture(graph, scope, task_id)?;
    let moment = stamp(payload);
    let correction = Records::new(graph, CORRECTION).create(
        scope,
        &json!({
            "id": service_id(),
            "task_id": task_id,
            "field_name": text(payload, "field_name"),
            "original_value": payload.get("original_value").cloned().unwrap_or(Value::Null),
            "corrected_value": payload.get("corrected_value").cloned().unwrap_or(Value::Null),
            "reason": text(payload, "reason"),
            "created_at": moment,
        }),
    )?;
    Records::new(graph, CAPTURE).update(
        scope,
        task_id,
        &json!({"status": "user_corrected", "updated_at": moment}),
    )?;
    Ok(correction)
}

/// `GET /tasks/{id}/corrections`: the corrections made against a capture.
pub fn corrections(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    task_id: &str,
) -> Result<Value, StoreError> {
    capture(graph, scope, task_id)?;
    let wanted = task_id.to_string();
    Ok(Value::Array(
        Records::new(graph, CORRECTION)
            .list(scope, 500)?
            .into_iter()
            .filter(|row| text(row, "task_id") == wanted)
            .collect(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::MemoryGraph;

    fn talked() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn capture_of(graph: &MemoryGraph, kind: &str) -> Value {
        create_capture(
            graph,
            &talked(),
            &json!({"input_type": kind, "input_url": "https://files.test/receipt.jpg", "now": 1000.0}),
        )
        .expect("a capture")
    }

    #[test]
    fn a_capture_is_one_of_five_kinds_and_one_of_seven_states() {
        let graph = MemoryGraph::new();
        let made = capture_of(&graph, "document");
        assert_eq!(made["input_type"], json!("document"));
        assert_eq!(made["status"], json!("received"));
        assert_eq!(made["errors"], json!([]));
        assert_eq!(made["metadata"], json!({}));
        // nothing here read the document, and the answer says so
        assert_eq!(made["reading_queued"], json!(false));

        assert!(matches!(
            create_capture(&graph, &talked(), &json!({"input_type": "hologram"})),
            Err(StoreError::Invalid(_))
        ));
        assert!(matches!(
            create_capture(
                &graph,
                &talked(),
                &json!({"input_type": "text", "status": "nearly"})
            ),
            Err(StoreError::Invalid(_))
        ));
        assert!(capture(&graph, &talked(), "nope").is_err());
    }

    #[test]
    fn the_inbox_is_the_same_rows_looked_at_by_state() {
        let graph = MemoryGraph::new();
        let waiting = capture_of(&graph, "image");
        let accepted = capture_of(&graph, "text");
        organize_capture(
            &graph,
            &talked(),
            &text(&accepted, "id"),
            &json!({"now": 2000.0}),
        )
        .expect("accept");
        let failed = capture_of(&graph, "audio");
        update_capture(
            &graph,
            &talked(),
            &text(&failed, "id"),
            &json!({"status": "failed"}),
        )
        .expect("a change");

        // everything not yet accepted reads as incomplete
        let open = inbox(&graph, &talked(), &json!({})).expect("inbox");
        assert_eq!(open.as_array().map(Vec::len), Some(2));
        // an accepted record reads as organized
        let done = inbox(&graph, &talked(), &json!({"view": "organized"})).expect("inbox");
        assert_eq!(done.as_array().map(Vec::len), Some(1));
        assert_eq!(done[0]["id"], accepted["id"]);
        assert_eq!(
            inbox(&graph, &talked(), &json!({"view": "all"}))
                .expect("inbox")
                .as_array()
                .map(Vec::len),
            Some(3)
        );

        // a fourth view is understood and refused, and the limit is held between one and five hundred
        assert!(matches!(
            inbox(&graph, &talked(), &json!({"view": "everything"})),
            Err(StoreError::Invalid(_))
        ));
        assert_eq!(
            inbox(&graph, &talked(), &json!({"view": "all", "limit": 0}))
                .expect("inbox")
                .as_array()
                .map(Vec::len),
            Some(1)
        );
        assert_eq!(
            inbox(&graph, &talked(), &json!({"view": "all", "limit": 9999}))
                .expect("inbox")
                .as_array()
                .map(Vec::len),
            Some(3)
        );

        // the badge counts each state, and totals what is open against what is accepted
        let summary = inbox_summary(&graph, &talked()).expect("summary");
        assert_eq!(summary["received"], json!(1));
        assert_eq!(summary["failed"], json!(1));
        assert_eq!(summary["completed"], json!(1));
        assert_eq!(summary["total_incomplete"], json!(2));
        assert_eq!(summary["total_organized"], json!(1));
        assert_eq!(summary["ai_extracted"], json!(0));
        assert!(summary["id"].is_null());
        assert_eq!(waiting["status"], json!("received"));
    }

    #[test]
    fn accepting_twice_changes_nothing_and_a_correction_marks_the_capture() {
        let graph = MemoryGraph::new();
        let made = capture_of(&graph, "document");
        let id = text(&made, "id");

        let first =
            organize_capture(&graph, &talked(), &id, &json!({"now": 2000.0})).expect("accept");
        assert_eq!(first["status"], json!("completed"));
        assert_eq!(first["processing_end_time"], json!(2000.0));

        // accepting an already accepted capture answers it unchanged, keeping the first finishing time
        let again =
            organize_capture(&graph, &talked(), &id, &json!({"now": 9000.0})).expect("accept");
        assert_eq!(again["processing_end_time"], json!(2000.0));

        // a correction is kept with the field and the value it started from, and marks the capture corrected
        let corrected = submit_correction(
            &graph,
            &talked(),
            &id,
            &json!({"field_name": "total_amount", "original_value": "41.32", "corrected_value": "43.12",
                    "reason": "the receipt says 43.12", "now": 3000.0}),
        )
        .expect("a correction");
        assert_eq!(corrected["task_id"], json!(id));
        assert_eq!(corrected["field_name"], json!("total_amount"));
        assert_eq!(corrected["original_value"], json!("41.32"));
        let held = capture(&graph, &talked(), &id).expect("a capture");
        // a completed capture a person overruled is no longer accepted as read
        assert_eq!(held["status"], json!("user_corrected"));

        let kept = corrections(&graph, &talked(), &id).expect("corrections");
        assert_eq!(kept.as_array().map(Vec::len), Some(1));

        // a delete removes the capture, and another Book sees none of this
        delete_capture(&graph, &talked(), &id).expect("a delete");
        assert!(capture(&graph, &talked(), &id).is_err());
        assert!(delete_capture(&graph, &talked(), &id).is_err());

        let elsewhere = BookScope::new("user-2", "book-2");
        let other = capture_of(&graph, "text");
        assert!(capture(&graph, &elsewhere, &text(&other, "id")).is_err());
        assert!(submit_correction(
            &graph,
            &elsewhere,
            &text(&other, "id"),
            &json!({"field_name": "x"})
        )
        .is_err());
        assert_eq!(
            captures(&graph, &elsewhere)
                .expect("captures")
                .as_array()
                .map(Vec::len),
            Some(0)
        );
        assert_eq!(
            inbox_summary(&graph, &elsewhere).expect("summary")["total_incomplete"],
            json!(0)
        );
    }
}
