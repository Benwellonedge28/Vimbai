#![forbid(unsafe_code)]
//! notifications-service: each person's own inbox, the preferences they set, and the templates a notice is filled from.
//!
//! Behaviours kept as shipped, each pinned by a test:
//!
//! * a notification is written once PER RECIPIENT and each copy belongs to that recipient, so a notice raised by one
//!   person lands in another's inbox and is theirs to read or delete. Every copy arrives already sent, with the moment
//!   it went, and remembers the sender it came from;
//! * an inbox is the caller's own: asking for somebody else's is refused outright rather than answered empty, and the
//!   answer carries the notifications (unread only when asked), how many are unread, and how many are held in all;
//! * marking one read, deleting one, and marking everything read work only on the caller's own copy - a copy that is
//!   not theirs is simply missing - and marking everything read counts only what was still unread;
//! * preferences are personal too, and a person who has never set any reads the defaults: no channel choices of their
//!   own, email batched every sixty minutes, no quiet hours;
//! * five templates come with the service. A notice built from one has its `{{variable}}` placeholders filled in, and a
//!   name the service does not know is missing rather than empty;
//! * a workflow notice is named after the event - started, completed, failed, cancelled - and is filed as a COMPLETED
//!   workflow only when it completed, everything else reading as a failure, which is what the shipped mapping did with
//!   its two-way choice; an approval notice is loud only when it is the request itself, and goes to the approvers, while
//!   every answer afterwards goes to the person who asked;
//! * the count a statistics question gives is the caller's own, and nobody is shown as listening, because there are no
//!   live sockets here - the shipped service broadcast each notice over one.

use std::collections::BTreeMap;

use serde_json::{json, Value};

use vimbai_store::read::{field, flag, optional, refused, text, whole};
use vimbai_store::{BookScope, Graph, RecordKind, Records, StoreError};

/// One person's copy of a notice.
pub const NOTICE: RecordKind = RecordKind::new("Notification", "OWNS_NOTIFICATION");

/// A person's preferences for their own inbox.
pub const PREFERENCES: RecordKind =
    RecordKind::new("NotificationPreference", "OWNS_NOTIFICATION_PREFERENCE");

/// The kinds of notice the service names.
pub const TYPES: [&str; 9] = [
    "approval_required",
    "approval_completed",
    "approval_rejected",
    "comment_added",
    "mention",
    "workflow_completed",
    "workflow_failed",
    "deadline_reminder",
    "system",
];

/// How loud a notice is.
pub const PRIORITIES: [&str; 4] = ["urgent", "high", "normal", "low"];

/// How a notice may be carried.
pub const CHANNELS: [&str; 5] = ["in_app", "email", "sms", "webhook", "push"];

/// The five templates the service ships with: a key, a kind, a subject, a body and the variables it expects.
pub const TEMPLATES: [(&str, &str, &str, &str, [&str; 4]); 5] = [
    (
        "approval_request",
        "approval_required",
        "Approval Required: {{title}}",
        "You have a new approval request for '{{title}}' from {{sender}}. Please review and take action.",
        ["title", "sender", "action_url", ""],
    ),
    (
        "approval_completed",
        "approval_completed",
        "Approved: {{title}}",
        "Your request '{{title}}' has been approved by {{approver}}.",
        ["title", "approver", "", ""],
    ),
    (
        "comment_added",
        "comment_added",
        "{{sender}} commented on {{title}}",
        "{{sender}} added a comment: {{comment}}",
        ["sender", "title", "comment", ""],
    ),
    (
        "mention",
        "mention",
        "{{sender}} mentioned you",
        "{{sender}} mentioned you in '{{title}}': {{comment}}",
        ["sender", "title", "comment", ""],
    ),
    (
        "deadline_reminder",
        "deadline_reminder",
        "Deadline Reminder: {{title}}",
        "Reminder: '{{title}}' is due on {{deadline}}.",
        ["title", "deadline", "", ""],
    ),
];

fn one_of(value: &str, allowed: &[&str], what: &str) -> Result<(), StoreError> {
    if allowed.contains(&value) {
        Ok(())
    } else {
        Err(refused(&format!(
            "{what} must be one of {}",
            allowed.join(", ")
        )))
    }
}

/// A list of words, given as a list or as the comma-separated form a query string carries.
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

fn notice_answer(record: &Value) -> Value {
    json!({
        "id": field(record, "id"),
        "type": field(record, "type"),
        "title": field(record, "title"),
        "message": field(record, "message"),
        "priority": field(record, "priority"),
        "recipients": words(record, "recipients"),
        "channels": words(record, "channels"),
        "metadata": record.get("metadata").cloned().unwrap_or(Value::Null),
        "action_url": optional(record, "action_url"),
        "scheduled_at": optional(record, "scheduled_at"),
        "expires_at": optional(record, "expires_at"),
        "sender": optional(record, "sender"),
        "owner": field(record, "owner"),
        "status": field(record, "status"),
        "created_at": field(record, "created_at"),
        "sent_at": optional(record, "sent_at"),
        "read_at": optional(record, "read_at"),
    })
}

/// The scope of one recipient's own inbox: their own, inside the Book the notice was raised in.
fn inbox(caller: &BookScope, recipient: &str) -> BookScope {
    BookScope::new(recipient, caller.book_id.clone())
}

/// Write one notice into every recipient's own inbox, and answer the copy as it was sent.
fn send(
    graph: &(impl Graph + ?Sized),
    caller: &BookScope,
    fields: &Value,
    recipients: &[String],
) -> Result<Value, StoreError> {
    let mut sent_any: Option<Value> = None;
    for recipient in recipients {
        let records = Records::new(graph, NOTICE);
        let copy = records.create(
            &inbox(caller, recipient),
            &json!({
                "id": vimbai_common::ids::service_id(),
                "type": field(fields, "type"),
                "title": field(fields, "title"),
                "message": field(fields, "message"),
                "priority": fields.get("priority").cloned().unwrap_or_else(|| json!("normal")),
                "recipients": recipients,
                "channels": fields.get("channels").cloned().unwrap_or_else(|| json!(["in_app"])),
                "metadata": fields.get("metadata").cloned().unwrap_or(Value::Null),
                "action_url": fields.get("action_url").cloned().unwrap_or(Value::Null),
                "scheduled_at": fields.get("scheduled_at").cloned().unwrap_or(Value::Null),
                "expires_at": fields.get("expires_at").cloned().unwrap_or(Value::Null),
                "sender": caller.user_id.clone(),
                "owner": recipient,
                // every copy goes out at once, as the shipped loop sent each one
                "status": "sent",
                "created_at": field(fields, "created_at"),
                "sent_at": field(fields, "created_at"),
            }),
        )?;
        sent_any = Some(notice_answer(&copy));
    }
    match sent_any {
        Some(sent) => Ok(sent),
        // a notice with no recipients is refused rather than written to nobody
        None => Err(refused("recipients must not be empty")),
    }
}

/// The fields a notice is made of, checked.
fn notice_fields(payload: &Value) -> Result<Value, StoreError> {
    let kind = text(payload, "type");
    one_of(&kind, &TYPES, "type")?;
    let priority = {
        let asked = text(payload, "priority");
        if asked.is_empty() {
            "normal".to_string()
        } else {
            asked
        }
    };
    one_of(&priority, &PRIORITIES, "priority")?;
    let channels = words(payload, "channels");
    for channel in &channels {
        one_of(channel, &CHANNELS, "channel")?;
    }
    let recipients = words(payload, "recipients");
    if recipients.is_empty() {
        return Err(refused("recipients must not be empty"));
    }
    Ok(json!({
        "type": kind,
        "title": text(payload, "title"),
        "message": text(payload, "message"),
        "priority": priority,
        "channels": if channels.is_empty() { vec!["in_app".to_string()] } else { channels },
        "metadata": payload.get("metadata").cloned().unwrap_or(Value::Null),
        "action_url": payload.get("action_url").cloned().unwrap_or(Value::Null),
        "scheduled_at": payload.get("scheduled_at").cloned().unwrap_or(Value::Null),
        "expires_at": payload.get("expires_at").cloned().unwrap_or(Value::Null),
        "created_at": text(payload, "created_at"),
    }))
}

/// `POST /notifications`: raise a notice into each recipient's inbox.
pub fn create_notification(
    graph: &(impl Graph + ?Sized),
    caller: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let fields = notice_fields(payload)?;
    let recipients = words(payload, "recipients");
    send(graph, caller, &fields, &recipients)
}

/// `POST /notifications/batch`: several notices at once.
pub fn create_notifications(
    graph: &(impl Graph + ?Sized),
    caller: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let empty = Vec::new();
    let asked = match payload.get("notifications") {
        Some(Value::Array(items)) => items.clone(),
        _ => empty,
    };
    let mut sent = Vec::new();
    for notice in &asked {
        sent.push(create_notification(graph, caller, notice)?);
    }
    Ok(json!({ "count": sent.len(), "notifications": sent }))
}

/// `GET /notifications/{user_id}`: a person's own inbox, and nobody else's.
pub fn inbox_of(
    graph: &(impl Graph + ?Sized),
    caller: &BookScope,
    user_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    if user_id != caller.user_id {
        return Err(StoreError::Forbidden("Access denied".to_string()));
    }
    let mut held: Vec<Value> = Records::new(graph, NOTICE)
        .list(&inbox(caller, user_id), 1000)?
        .into_iter()
        .filter(|notice| field(notice, "owner") == user_id)
        .collect();
    // newest first, as a listing of notices reads
    held.sort_by(|left, right| field(right, "created_at").cmp(field(left, "created_at")));
    let unread = held.iter().filter(|notice| !is_read(notice)).count();
    let total = held.len();
    let kept: Vec<Value> = if flag(payload, "unread_only") {
        held.iter()
            .filter(|notice| !is_read(notice))
            .cloned()
            .collect()
    } else {
        held
    };
    let limit = {
        let asked = whole(payload, "limit") as usize;
        if asked == 0 {
            50
        } else {
            asked.min(200)
        }
    };
    let page: Vec<Value> = kept.iter().take(limit).map(notice_answer).collect();
    Ok(json!({
        "notifications": page,
        "unread_count": unread,
        "total_count": total,
    }))
}

fn is_read(notice: &Value) -> bool {
    field(notice, "status") == "read"
}

/// `PUT /notifications/{id}/read`: the caller's own copy, marked read.
pub fn mark_read(
    graph: &(impl Graph + ?Sized),
    caller: &BookScope,
    notification_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let records = Records::new(graph, NOTICE);
    let held = records.get(caller, notification_id)?;
    let mine = field(&held, "owner") == caller.user_id;
    if !mine {
        return Err(StoreError::NotFound);
    }
    records.update(
        caller,
        field(&held, "id"),
        &json!({"status": "read", "read_at": text(payload, "read_at")}),
    )?;
    Ok(json!({"status": "success", "notification_id": notification_id}))
}

/// `PUT /notifications/{user_id}/read-all`: everything still unread in a person's own inbox.
pub fn mark_all_read(
    graph: &(impl Graph + ?Sized),
    caller: &BookScope,
    user_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    if user_id != caller.user_id {
        return Err(StoreError::Forbidden("Access denied".to_string()));
    }
    let records = Records::new(graph, NOTICE);
    let held: Vec<Value> = records
        .list(&inbox(caller, user_id), 1000)?
        .into_iter()
        .filter(|notice| field(notice, "owner") == user_id)
        .collect();
    let mut marked = 0_i64;
    for notice in &held {
        if is_read(notice) {
            continue;
        }
        records.update(
            &inbox(caller, user_id),
            field(notice, "id"),
            &json!({"status": "read", "read_at": text(payload, "read_at")}),
        )?;
        marked += 1;
    }
    Ok(json!({"status": "success", "marked_count": marked}))
}

/// `DELETE /notifications/{id}`: the caller's own copy, gone.
pub fn delete_notification(
    graph: &(impl Graph + ?Sized),
    caller: &BookScope,
    notification_id: &str,
) -> Result<Value, StoreError> {
    let records = Records::new(graph, NOTICE);
    let held = records.get(caller, notification_id)?;
    if field(&held, "owner") != caller.user_id {
        return Err(StoreError::NotFound);
    }
    records.delete(caller, notification_id)?;
    Ok(json!({"status": "success"}))
}

/// `PUT /preferences/{user_id}`: a person's own preferences, replaced whole.
pub fn save_preferences(
    graph: &(impl Graph + ?Sized),
    caller: &BookScope,
    user_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    if user_id != caller.user_id {
        return Err(StoreError::Forbidden("Access denied".to_string()));
    }
    let records = Records::new(graph, PREFERENCES);
    let mut fields = json!({
        "owner": user_id,
        "channels": payload.get("channels").cloned().unwrap_or_else(|| json!({})),
        "quiet_hours_start": optional(payload, "quiet_hours_start"),
        "quiet_hours_end": optional(payload, "quiet_hours_end"),
        "email_batch": match payload.get("email_batch") {
            None | Some(Value::Null) => true,
            Some(_) => flag(payload, "email_batch"),
        },
        "email_batch_interval_minutes": interval_of(payload),
    });
    fields["updated_at"] = json!(text(payload, "updated_at"));
    // a person has one set of preferences: saving again replaces them rather than piling up
    let existing = records.list(caller, 1000)?;
    let saved = match existing.iter().find(|held| field(held, "owner") == user_id) {
        Some(held) => records.update(caller, field(held, "id"), &fields)?,
        None => {
            fields["id"] = json!(vimbai_common::ids::service_id());
            records.create(caller, &fields)?
        }
    };
    Ok(json!({"status": "success", "preferences": preferences_answer(&saved)}))
}

/// How often email is batched: an hour when nothing is said.
fn interval_of(payload: &Value) -> u32 {
    let asked = whole(payload, "email_batch_interval_minutes");
    if asked == 0 {
        60
    } else {
        asked
    }
}

fn preferences_answer(record: &Value) -> Value {
    json!({
        "user_id": field(record, "owner"),
        "channels": record.get("channels").cloned().unwrap_or_else(|| json!({})),
        "quiet_hours_start": optional(record, "quiet_hours_start"),
        "quiet_hours_end": optional(record, "quiet_hours_end"),
        "email_batch": flag(record, "email_batch"),
        "email_batch_interval_minutes": whole(record, "email_batch_interval_minutes"),
    })
}

/// `GET /preferences/{user_id}`: what a person set, or the defaults when they never did.
pub fn preferences_of(
    graph: &(impl Graph + ?Sized),
    caller: &BookScope,
    user_id: &str,
) -> Result<Value, StoreError> {
    if user_id != caller.user_id {
        return Err(StoreError::Forbidden("Access denied".to_string()));
    }
    let held = Records::new(graph, PREFERENCES).list(caller, 1000)?;
    match held.iter().find(|record| field(record, "owner") == user_id) {
        Some(record) => Ok(preferences_answer(record)),
        // never set: no channel choices, email batched every hour, no quiet hours
        None => Ok(json!({
            "user_id": user_id,
            "channels": {},
            "quiet_hours_start": Value::Null,
            "quiet_hours_end": Value::Null,
            "email_batch": true,
            "email_batch_interval_minutes": 60,
        })),
    }
}

/// `GET /templates`: the templates the service comes with.
pub fn notification_templates() -> Value {
    Value::Array(
        TEMPLATES
            .iter()
            .map(|(key, kind, subject, body, variables)| {
                json!({
                    "name": key,
                    "template": template_answer(key, kind, subject, body, variables),
                })
            })
            .collect(),
    )
}

fn template_answer(
    key: &str,
    kind: &str,
    subject: &str,
    body: &str,
    variables: &[&str; 4],
) -> Value {
    json!({
        "name": key,
        "type": kind,
        "subject_template": subject,
        "body_template": body,
        "variables": variables.iter().filter(|name| !name.is_empty()).collect::<Vec<_>>(),
    })
}

fn template_named(
    template_name: &str,
) -> Option<(
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    [&'static str; 4],
)> {
    TEMPLATES
        .iter()
        .find(|(key, _, _, _, _)| *key == template_name)
        .copied()
}

/// `GET /templates/{name}`.
pub fn template_held(template_name: &str) -> Result<Value, StoreError> {
    match template_named(template_name) {
        Some((key, kind, subject, body, variables)) => {
            Ok(template_answer(key, kind, subject, body, &variables))
        }
        None => Err(StoreError::NotFound),
    }
}

/// `POST /templates/{name}/send`: a notice built from a template, its placeholders filled in.
pub fn send_from_template(
    graph: &(impl Graph + ?Sized),
    caller: &BookScope,
    template_name: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let (_, kind, subject, body, _) = template_named(template_name).ok_or(StoreError::NotFound)?;
    let variables = match payload.get("variables") {
        Some(Value::Object(map)) => map.clone(),
        _ => serde_json::Map::new(),
    };
    let fill = |template: &str| -> String {
        let mut filled = template.to_string();
        for (name, value) in &variables {
            let told = value.as_str().unwrap_or_default().to_string();
            filled = filled.replace(&format!("{{{{{name}}}}}"), &told);
        }
        filled
    };
    let recipients = words(payload, "recipients");
    if recipients.is_empty() {
        return Err(refused("recipients must not be empty"));
    }
    let fields = json!({
        "type": kind,
        "title": fill(subject),
        "message": fill(body),
        "priority": "normal",
        "channels": ["in_app", "email"],
        "created_at": text(payload, "created_at"),
    });
    send(graph, caller, &fields, &recipients)
}

/// `POST /workflow/{id}/notify`: a workflow's progress told to everyone taking part.
pub fn notify_workflow(
    graph: &(impl Graph + ?Sized),
    caller: &BookScope,
    workflow_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let event = text(payload, "event_type");
    let (title, message) = match event.as_str() {
        "started" => (
            "Workflow Started",
            format!("Workflow {workflow_id} has been initiated."),
        ),
        "completed" => (
            "Workflow Completed",
            format!("Workflow {workflow_id} has been completed successfully."),
        ),
        "failed" => (
            "Workflow Failed",
            format!("Workflow {workflow_id} has failed."),
        ),
        "cancelled" => (
            "Workflow Cancelled",
            format!("Workflow {workflow_id} has been cancelled."),
        ),
        _ => {
            return Err(refused(
                "event_type must be started, completed, failed or cancelled",
            ))
        }
    };
    // the shipped mapping had two kinds to choose from: only a completion reads as completed
    let kind = if event == "completed" {
        "workflow_completed"
    } else {
        "workflow_failed"
    };
    let mut metadata = match payload.get("metadata") {
        Some(Value::Object(map)) => map.clone(),
        _ => serde_json::Map::new(),
    };
    metadata.insert("workflow_id".to_string(), json!(workflow_id));
    metadata.insert("event_type".to_string(), json!(event));
    let recipients = words(payload, "participants");
    let fields = json!({
        "type": kind,
        "title": title,
        "message": message,
        "priority": "normal",
        "channels": ["in_app"],
        "metadata": Value::Object(metadata),
        "action_url": format!("/workflows/{workflow_id}"),
        "created_at": text(payload, "created_at"),
    });
    send(graph, caller, &fields, &recipients)
}

/// `POST /approval/{id}/notify`: an approval asked for, answered, or commented on.
pub fn notify_approval(
    graph: &(impl Graph + ?Sized),
    caller: &BookScope,
    approval_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let event = text(payload, "event_type");
    let requester = text(payload, "requester");
    let (kind, title, message, priority) = match event.as_str() {
        "requested" => (
            "approval_required",
            "Approval Required",
            format!("New approval request {approval_id}"),
            "high",
        ),
        "approved" => (
            "approval_completed",
            "Request Approved",
            format!("Your request {approval_id} has been approved"),
            "normal",
        ),
        "rejected" => (
            "approval_rejected",
            "Request Rejected",
            format!("Your request {approval_id} has been rejected"),
            "normal",
        ),
        "commented" => (
            "comment_added",
            "Comment Added",
            format!("New comment on approval {approval_id}"),
            "normal",
        ),
        _ => ("system", "Approval Update", String::new(), "normal"),
    };
    // the request goes to the approvers; every answer afterwards goes to whoever asked
    let recipients = if event == "requested" {
        words(payload, "approvers")
    } else {
        vec![requester.clone()]
    };
    let mut metadata = match payload.get("metadata") {
        Some(Value::Object(map)) => map.clone(),
        _ => serde_json::Map::new(),
    };
    metadata.insert("approval_id".to_string(), json!(approval_id));
    metadata.insert("requester".to_string(), json!(requester));
    let fields = json!({
        "type": kind,
        "title": title,
        "message": message,
        "priority": priority,
        "channels": ["in_app", "email"],
        "metadata": Value::Object(metadata),
        "action_url": format!("/approvals/{approval_id}"),
        "created_at": text(payload, "created_at"),
    });
    send(graph, caller, &fields, &recipients)
}

/// `GET /stats`: what the caller's own inbox holds.
pub fn notification_totals(
    graph: &(impl Graph + ?Sized),
    caller: &BookScope,
) -> Result<Value, StoreError> {
    let mut by_status: BTreeMap<String, i64> = BTreeMap::new();
    let held: Vec<Value> = Records::new(graph, NOTICE)
        .list(caller, 1000)?
        .into_iter()
        .filter(|notice| field(notice, "owner") == caller.user_id)
        .collect();
    for notice in &held {
        *by_status
            .entry(field(notice, "status").to_string())
            .or_insert(0) += 1;
    }
    let unread = held.iter().filter(|notice| !is_read(notice)).count();
    Ok(json!({
        "total_notifications": held.len(),
        "unread_notifications": unread,
        "by_status": by_status,
        // nobody is listening: the shipped service kept its sockets in memory and none exist here
        "active_connections": 0,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::MemoryGraph;

    fn mine() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn a_notice(graph: &MemoryGraph, recipients: Value) -> Value {
        create_notification(
            graph,
            &mine(),
            &json!({
                "type": "approval_required",
                "title": "Invoice needs approval",
                "message": "INV-1 is waiting",
                "recipients": recipients,
                "created_at": "2026-01-02T09:00:00Z",
            }),
        )
        .expect("a notice")
    }

    #[test]
    fn a_notice_lands_in_every_recipients_own_inbox() {
        let graph = MemoryGraph::new();
        let sent = a_notice(&graph, json!(["user-2", "user-3"]));
        assert_eq!(sent["status"], json!("sent"));
        assert_eq!(sent["sender"], json!("user-1"));
        assert_eq!(sent["channels"], json!(["in_app"]));

        // each recipient finds their own copy, and it is theirs to read
        let theirs = BookScope::new("user-2", "book-1");
        let inbox = inbox_of(&graph, &theirs, "user-2", &json!({})).expect("an inbox");
        assert_eq!(inbox["total_count"], json!(1));
        assert_eq!(inbox["unread_count"], json!(1));
        let other = BookScope::new("user-3", "book-1");
        let other_inbox = inbox_of(&graph, &other, "user-3", &json!({})).expect("an inbox");
        assert_eq!(other_inbox["total_count"], json!(1));
        // and the sender's own inbox was never written to
        let mine_inbox = inbox_of(&graph, &mine(), "user-1", &json!({})).expect("an inbox");
        assert_eq!(mine_inbox["total_count"], json!(0));
    }

    #[test]
    fn an_inbox_is_the_callers_own() {
        let graph = MemoryGraph::new();
        a_notice(&graph, json!(["user-2"]));
        // asking for somebody else's inbox is refused outright, not answered empty
        assert!(matches!(
            inbox_of(&graph, &mine(), "user-2", &json!({})),
            Err(StoreError::Forbidden(_))
        ));
        assert!(matches!(
            mark_all_read(&graph, &mine(), "user-2", &json!({})),
            Err(StoreError::Forbidden(_))
        ));
        // and a notice that is not the caller's is simply missing
        let theirs = BookScope::new("user-2", "book-1");
        let their_notice = inbox_of(&graph, &theirs, "user-2", &json!({})).expect("an inbox");
        let id = their_notice["notifications"][0]["id"]
            .as_str()
            .expect("an id")
            .to_string();
        assert!(mark_read(&graph, &mine(), &id, &json!({})).is_err());
        assert!(delete_notification(&graph, &mine(), &id).is_err());
    }

    #[test]
    fn reading_and_deleting_only_touch_what_is_still_unread() {
        let graph = MemoryGraph::new();
        a_notice(&graph, json!(["user-1"]));
        a_notice(&graph, json!(["user-1"]));
        let inbox = inbox_of(&graph, &mine(), "user-1", &json!({})).expect("an inbox");
        assert_eq!(inbox["unread_count"], json!(2));

        let first = inbox["notifications"][0]["id"]
            .as_str()
            .expect("an id")
            .to_string();
        mark_read(
            &graph,
            &mine(),
            &first,
            &json!({"read_at": "2026-01-03T09:00:00Z"}),
        )
        .expect("read");
        let after_one = inbox_of(&graph, &mine(), "user-1", &json!({})).expect("an inbox");
        assert_eq!(after_one["unread_count"], json!(1));
        // unread only narrows the listing, not the counts
        let unread_only =
            inbox_of(&graph, &mine(), "user-1", &json!({"unread_only": true})).expect("an inbox");
        assert_eq!(
            unread_only["notifications"]
                .as_array()
                .map(|held| held.len()),
            Some(1)
        );
        assert_eq!(unread_only["total_count"], json!(2));

        // marking everything read counts only what was left
        let marked = mark_all_read(
            &graph,
            &mine(),
            "user-1",
            &json!({"read_at": "2026-01-04T09:00:00Z"}),
        )
        .expect("read all");
        assert_eq!(marked["marked_count"], json!(1));

        delete_notification(&graph, &mine(), &first).expect("deleted");
        let after_delete = inbox_of(&graph, &mine(), "user-1", &json!({})).expect("an inbox");
        assert_eq!(after_delete["total_count"], json!(1));
    }

    #[test]
    fn a_notice_of_a_kind_the_service_does_not_name_is_refused() {
        let graph = MemoryGraph::new();
        assert!(create_notification(
            &graph,
            &mine(),
            &json!({"type": "gossip", "title": "T", "message": "M", "recipients": ["user-2"]})
        )
        .is_err());
        // and a notice addressed to nobody is refused rather than written to nobody
        assert!(create_notification(
            &graph,
            &mine(),
            &json!({"type": "system", "title": "T", "message": "M", "recipients": []})
        )
        .is_err());
    }

    #[test]
    fn preferences_default_until_a_person_sets_their_own() {
        let graph = MemoryGraph::new();
        let defaults = preferences_of(&graph, &mine(), "user-1").expect("preferences");
        assert_eq!(defaults["email_batch"], json!(true));
        assert_eq!(defaults["email_batch_interval_minutes"], json!(60));
        assert_eq!(defaults["channels"], json!({}));

        let saved = save_preferences(
            &graph,
            &mine(),
            "user-1",
            &json!({"quiet_hours_start": "20:00", "quiet_hours_end": "07:00",
                    "email_batch": false, "updated_at": "2026-01-02T09:00:00Z"}),
        )
        .expect("saved");
        assert_eq!(saved["preferences"]["email_batch"], json!(false));
        assert_eq!(saved["preferences"]["quiet_hours_start"], json!("20:00"));

        // saving again replaces rather than piling up
        save_preferences(&graph, &mine(), "user-1", &json!({"email_batch": true})).expect("saved");
        let held = Records::new(&graph, PREFERENCES)
            .list(&mine(), 1000)
            .expect("preferences");
        assert_eq!(held.len(), 1);
        let after = preferences_of(&graph, &mine(), "user-1").expect("preferences");
        assert_eq!(after["email_batch"], json!(true));

        // another person's preferences are not the caller's to read
        assert!(matches!(
            preferences_of(&graph, &mine(), "user-2"),
            Err(StoreError::Forbidden(_))
        ));
    }

    #[test]
    fn a_template_fills_its_placeholders_and_an_unknown_name_is_missing() {
        let graph = MemoryGraph::new();
        let listed = notification_templates();
        assert_eq!(listed.as_array().map(|held| held.len()), Some(5));
        assert!(template_held("approval_request").is_ok());
        assert!(template_held("invented").is_err());

        let sent = send_from_template(
            &graph,
            &mine(),
            "comment_added",
            &json!({
                "recipients": ["user-2"],
                "variables": {"sender": "Tariro", "title": "Invoice 7", "comment": "looks wrong"},
                "created_at": "2026-01-02T09:00:00Z"
            }),
        )
        .expect("a notice");
        assert_eq!(sent["title"], json!("Tariro commented on Invoice 7"));
        assert_eq!(
            sent["message"],
            json!("Tariro added a comment: looks wrong")
        );
        assert_eq!(sent["channels"], json!(["in_app", "email"]));
        assert_eq!(sent["type"], json!("comment_added"));
    }

    #[test]
    fn a_workflow_tells_only_a_completion_as_completed() {
        let graph = MemoryGraph::new();
        let started = notify_workflow(
            &graph,
            &mine(),
            "wf-1",
            &json!({"event_type": "started", "participants": ["user-2"],
                    "created_at": "2026-01-02T09:00:00Z"}),
        )
        .expect("a notice");
        // the shipped mapping filed everything that was not a completion as a failure
        assert_eq!(started["type"], json!("workflow_failed"));
        assert_eq!(started["title"], json!("Workflow Started"));
        assert_eq!(started["action_url"], json!("/workflows/wf-1"));
        assert_eq!(started["metadata"]["workflow_id"], json!("wf-1"));

        let done = notify_workflow(
            &graph,
            &mine(),
            "wf-1",
            &json!({"event_type": "completed", "participants": ["user-2"],
                    "created_at": "2026-01-02T10:00:00Z"}),
        )
        .expect("a notice");
        assert_eq!(done["type"], json!("workflow_completed"));
        // and an event the service does not name is refused
        assert!(notify_workflow(
            &graph,
            &mine(),
            "wf-1",
            &json!({"event_type": "paused", "participants": ["user-2"]})
        )
        .is_err());
    }

    #[test]
    fn an_approval_goes_to_the_approvers_and_its_answer_to_the_requester() {
        let graph = MemoryGraph::new();
        let asked = notify_approval(
            &graph,
            &mine(),
            "ap-1",
            &json!({"event_type": "requested", "requester": "user-1", "approvers": ["user-2"],
                    "created_at": "2026-01-02T09:00:00Z"}),
        )
        .expect("a notice");
        assert_eq!(asked["type"], json!("approval_required"));
        assert_eq!(asked["priority"], json!("high"));

        let approved = notify_approval(
            &graph,
            &mine(),
            "ap-1",
            &json!({"event_type": "approved", "requester": "user-1", "approvers": ["user-2"],
                    "created_at": "2026-01-02T10:00:00Z"}),
        )
        .expect("a notice");
        assert_eq!(approved["type"], json!("approval_completed"));
        assert_eq!(approved["priority"], json!("normal"));
        // the answer went to the requester, so it is in their own inbox
        let theirs = inbox_of(&graph, &mine(), "user-1", &json!({})).expect("an inbox");
        assert_eq!(theirs["total_count"], json!(1));

        let counted = notification_totals(&graph, &mine()).expect("a count");
        assert_eq!(counted["total_notifications"], json!(1));
        assert_eq!(counted["active_connections"], json!(0));
    }
}
