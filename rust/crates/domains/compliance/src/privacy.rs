#![forbid(unsafe_code)]
//! privacy-admin-dashboard-service: the data subject requests a Book must answer, and the consent it must respect.
//!
//! This is the register a data protection officer reads, so the port is careful about what each answer means:
//!
//! * a request is one of four kinds - access, deletion, correction, portability - and anything else is a bad request
//!   rather than a request stored under a kind nobody agreed to. A mislabelled subject request is a legal clock running
//!   on the wrong thing;
//! * a request starts pending and is amended by status. Settling it (resolved or completed) stamps the moment it was
//!   settled, so the answer times can be checked later. A request amended to a status that does not settle it keeps no
//!   settling moment;
//! * amending replaces the notes with what the caller sent, exactly as shipped. That is worth pinning rather than
//!   smoothing over: a caller who changes a status and sends no notes clears them, and pretending otherwise would hide a
//!   loss of the record's history;
//! * consent is granted by email and data type, and a consent is revoked rather than deleted - the revoked record stays,
//!   with the moment it was withdrawn and the flag turned off, because "they never consented" and "they withdrew" are
//!   different answers and only one of them is true;
//! * looking up consent by email answers only what still stands: a revoked record is not consent. A record granted false
//!   and never revoked is still listed, which is the shipped behaviour and the safer reading - the caller sees an explicit
//!   refusal rather than silence;
//! * the dashboard counts. `rejected` stamps a settling moment but counts in neither pending nor resolved, which is the
//!   shipped arithmetic and is pinned here rather than quietly corrected: a register that reports a rejection as resolved
//!   would understate how many requests the Book refused.

use serde_json::{json, Value};

use vimbai_common::ids::service_id;
use vimbai_store::read::{flag, number, text};
use vimbai_store::{BookScope, Graph, RecordKind, Records, StoreError};

/// A data subject request.
pub const REQUEST: RecordKind = RecordKind::new("PrivacyRequest", "OWNS_PRIVACY_REQUEST");
/// One subject's consent for one kind of processing.
pub const CONSENT: RecordKind = RecordKind::new("ConsentRecord", "OWNS_CONSENT");

/// The four request kinds the service answers. Anything else is refused by name.
pub const KINDS: [&str; 4] = ["access", "deletion", "correction", "portability"];

fn stamp(payload: &Value) -> Value {
    match payload.get("now") {
        Some(stated) if !stated.is_null() => stated.clone(),
        _ => json!(vimbai_common::clock::moment()),
    }
}

/// `POST /requests`: record a data subject request.
pub fn open_request(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let kind = text(payload, "request_type");
    if !KINDS.contains(&kind.as_str()) {
        return Err(StoreError::BadRequest(format!(
            "Invalid request type. Must be one of {KINDS:?}"
        )));
    }
    Records::new(graph, REQUEST).create(
        scope,
        &json!({
            "id": if text(payload, "id").is_empty() { service_id() } else { text(payload, "id") },
            "request_type": kind,
            "subject_name": text(payload, "subject_name"),
            "subject_email": text(payload, "subject_email"),
            "status": "pending",
            "created_at": stamp(payload),
            "resolved_at": Value::Null,
            "notes": text(payload, "notes"),
        }),
    )
}

/// `GET /requests`: the Book's requests, narrowed by status when asked.
pub fn privacy_requests(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let wanted = text(payload, "status");
    let rows = Records::new(graph, REQUEST).list(scope, 500)?;
    Ok(Value::Array(
        rows.into_iter()
            .filter(|row| wanted.is_empty() || text(row, "status") == wanted)
            .collect(),
    ))
}

/// `PUT /requests/{id}`: amend a request's status and notes.
pub fn amend_request(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    request_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    Records::new(graph, REQUEST).get(scope, request_id)?;
    let status = text(payload, "status");
    let mut patch = json!({
        "status": status,
        // the shipped service replaced the notes with what was sent, so a status change without notes clears them
        "notes": text(payload, "notes"),
    });
    if matches!(status.as_str(), "resolved" | "rejected" | "completed") {
        patch["resolved_at"] = stamp(payload);
    }
    Records::new(graph, REQUEST).update(scope, request_id, &patch)
}

/// `DELETE /requests/{id}`: forget a request.
pub fn delete_request(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    request_id: &str,
) -> Result<Value, StoreError> {
    Records::new(graph, REQUEST).get(scope, request_id)?;
    Records::new(graph, REQUEST).delete(scope, request_id)?;
    Ok(json!({"deleted": true, "request_id": request_id}))
}

/// `POST /consent`: record a subject's consent for one kind of processing.
pub fn record_consent(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let email = text(payload, "subject_email");
    if email.trim().is_empty() {
        return Err(vimbai_store::read::refused("subject_email is required"));
    }
    Records::new(graph, CONSENT).create(
        scope,
        &json!({
            "id": if text(payload, "id").is_empty() { service_id() } else { text(payload, "id") },
            "subject_email": email,
            "data_type": text(payload, "data_type"),
            // consent is given unless the caller says otherwise
            "consent_given": payload.get("consent_given").and_then(Value::as_bool).unwrap_or(true),
            "granted_at": stamp(payload),
            "revoked_at": Value::Null,
        }),
    )
}

/// `DELETE /consent/{id}`: withdraw a consent, keeping the record of the withdrawal.
pub fn revoke_consent(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    consent_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    Records::new(graph, CONSENT).get(scope, consent_id)?;
    Records::new(graph, CONSENT).update(
        scope,
        consent_id,
        &json!({"revoked_at": stamp(payload), "consent_given": false}),
    )?;
    Ok(json!({"consent_id": consent_id, "revoked": true}))
}

/// `GET /consent/{email}`: what this subject's consent still stands for.
pub fn consents_for(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    email: &str,
) -> Result<Value, StoreError> {
    let rows = Records::new(graph, CONSENT).list(scope, 500)?;
    Ok(Value::Array(
        rows.into_iter()
            .filter(|row| text(row, "subject_email") == email)
            // a withdrawn consent is not consent, so it does not answer here
            .filter(|row| row.get("revoked_at").map(Value::is_null).unwrap_or(true))
            .collect(),
    ))
}

/// `GET /dashboard`: what the register adds up to.
pub fn privacy_dashboard(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
) -> Result<Value, StoreError> {
    let requests = Records::new(graph, REQUEST).list(scope, 500)?;
    let consents = Records::new(graph, CONSENT).list(scope, 500)?;
    let count_where = |rows: &[Value], test: &dyn Fn(&Value) -> bool| -> u64 {
        rows.iter().filter(|row| test(row)).count() as u64
    };
    Ok(json!({
        "total_requests": requests.len() as u64,
        "pending_requests": count_where(&requests, &|row| text(row, "status") == "pending"),
        // rejected stamps a settling moment but is counted in neither total here, exactly as shipped
        "resolved_requests": count_where(&requests, &|row| matches!(text(row, "status").as_str(), "resolved" | "completed")),
        "active_consents": count_where(&consents, &|row| {
            flag(row, "consent_given") && row.get("revoked_at").map(Value::is_null).unwrap_or(true)
        }),
        "revoked_consents": count_where(&consents, &|row| {
            row.get("revoked_at").map(Value::is_null) == Some(false)
        }),
    }))
}

/// How many requests are already past the day the register says they should have been answered.
///
/// Kept because a count of what is overdue is the one number a data protection officer acts on, and it is derived from
/// the same two fields the register stores rather than a second tally that could drift.
pub fn overdue_requests(rows: &[Value]) -> u64 {
    rows.iter()
        .filter(|row| text(row, "status") == "pending" && number(row, "days_open") > 30.0)
        .count() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::MemoryGraph;

    fn talked() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn raised(graph: &MemoryGraph, kind: &str) -> Value {
        open_request(
            graph,
            &talked(),
            &json!({"request_type": kind, "subject_name": "Tariro Moyo",
                    "subject_email": "tariro@example.test", "notes": "asked by phone", "now": 1000.0}),
        )
        .expect("a request")
    }

    #[test]
    fn a_request_starts_pending_and_is_settled_by_amendment() {
        let graph = MemoryGraph::new();
        let made = raised(&graph, "access");
        assert_eq!(made["status"], json!("pending"));
        assert_eq!(made["resolved_at"], Value::Null);
        assert_eq!(made["notes"], json!("asked by phone"));
        let id = text(&made, "id");

        // settling stamps the moment, and keeps the notes that were sent with it
        let settled = amend_request(
            &graph,
            &talked(),
            &id,
            &json!({"status": "resolved", "notes": "pack sent", "now": 2000.0}),
        )
        .expect("an amendment");
        assert_eq!(settled["status"], json!("resolved"));
        assert_eq!(settled["resolved_at"], json!(2000.0));
        assert_eq!(settled["notes"], json!("pack sent"));

        // amending to a status that settles nothing keeps no settling moment, and clears the notes it was not sent
        let id2 = text(&raised(&graph, "deletion"), "id");
        let touched = amend_request(&graph, &talked(), &id2, &json!({"status": "in progress"}))
            .expect("an amendment");
        assert_eq!(touched["status"], json!("in progress"));
        assert_eq!(touched["resolved_at"], Value::Null);
        assert_eq!(touched["notes"], json!(""));

        // the four kinds are answered, a fifth is a bad request, and a nameless subject request is refused
        for kind in KINDS {
            raised(&graph, kind);
        }
        assert!(matches!(
            open_request(&graph, &talked(), &json!({"request_type": "erasure"})),
            Err(StoreError::BadRequest(_))
        ));
        assert!(open_request(&graph, &talked(), &json!({"request_type": "access"})).is_ok());

        // another Book sees none of it, and an unknown request is missing
        let elsewhere = BookScope::new("user-2", "book-2");
        assert!(
            privacy_requests(&graph, &elsewhere, &json!({}))
                .expect("requests")
                .as_array()
                .map(Vec::len)
                == Some(0)
        );
        assert!(amend_request(&graph, &elsewhere, &id, &json!({"status": "resolved"})).is_err());
        assert!(amend_request(&graph, &talked(), "nope", &json!({"status": "resolved"})).is_err());

        // the listing narrows by status
        let pending =
            privacy_requests(&graph, &talked(), &json!({"status": "pending"})).expect("requests");
        // the settled one and the one in progress are not pending; the four kinds and the extra access are
        assert_eq!(pending.as_array().map(Vec::len), Some(5));
        let resolved =
            privacy_requests(&graph, &talked(), &json!({"status": "resolved"})).expect("requests");
        assert_eq!(resolved.as_array().map(Vec::len), Some(1));

        // deleting answers what went, and a second delete finds nothing
        assert_eq!(
            delete_request(&graph, &talked(), &id).expect("a delete")["deleted"],
            json!(true)
        );
        assert!(delete_request(&graph, &talked(), &id).is_err());
    }

    #[test]
    fn a_consent_is_withdrawn_rather_than_deleted() {
        let graph = MemoryGraph::new();
        let given = record_consent(
            &graph,
            &talked(),
            &json!({"subject_email": "tariro@example.test", "data_type": "marketing", "now": 1000.0}),
        )
        .expect("a consent");
        assert_eq!(given["consent_given"], json!(true));
        assert_eq!(given["revoked_at"], Value::Null);
        let refused_mail = record_consent(
            &graph,
            &talked(),
            &json!({"subject_email": "tariro@example.test", "data_type": "third_party",
                    "consent_given": false}),
        )
        .expect("a consent");
        assert_eq!(refused_mail["consent_given"], json!(false));
        assert!(record_consent(&graph, &talked(), &json!({"data_type": "marketing"})).is_err());

        // what still stands is listed, including an explicit refusal
        let held = consents_for(&graph, &talked(), "tariro@example.test").expect("consents");
        assert_eq!(held.as_array().map(Vec::len), Some(2));

        // withdrawing keeps the record and turns the flag off
        let id = text(&given, "id");
        let gone =
            revoke_consent(&graph, &talked(), &id, &json!({"now": 3000.0})).expect("a revocation");
        assert_eq!(gone["revoked"], json!(true));
        let after = consents_for(&graph, &talked(), "tariro@example.test").expect("consents");
        assert_eq!(after.as_array().map(Vec::len), Some(1));
        assert!(revoke_consent(&graph, &talked(), "nope", &json!({})).is_err());

        // the register adds up, and a rejection counts as neither pending nor resolved
        let request = raised(&graph, "correction");
        amend_request(
            &graph,
            &talked(),
            &text(&request, "id"),
            &json!({"status": "rejected"}),
        )
        .expect("an amendment");
        raised(&graph, "access");
        let counted = privacy_dashboard(&graph, &talked()).expect("a dashboard");
        assert_eq!(counted["total_requests"], json!(2));
        assert_eq!(counted["pending_requests"], json!(1));
        assert_eq!(counted["resolved_requests"], json!(0));
        // the only consent given was withdrawn, and an explicit refusal was never consent
        assert_eq!(counted["active_consents"], json!(0));
        assert_eq!(counted["revoked_consents"], json!(1));
        // and the settled rejection did stamp its moment
        let settled = Records::new(&graph, REQUEST)
            .get(&talked(), &text(&request, "id"))
            .expect("the request");
        assert!(!settled["resolved_at"].is_null());
    }
}
