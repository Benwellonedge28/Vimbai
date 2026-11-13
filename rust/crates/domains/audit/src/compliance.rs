#![forbid(unsafe_code)]
//! audit-compliance-service: the immutable trail a Book keeps, the versions of a resource, the lineage those
//! events trace, the integrity of their chain, and the reports and assessments drawn from them.
//!
//! Events and version snapshots are records on the store, Book-scoped. Everything else here is answered from
//! those records or from the caller's own figures and is not stored, which is what the shipped service did.
//!
//! Behaviours kept as shipped, each pinned by a test:
//!
//! * an event's checksum is taken over the event body with the checksum left out, by one canonical serializer -
//!   keys in sorted order and the body as JSON - so the same body always hashes the same way. The shipped service
//!   had a latent fault here (it hashed one rendering and verified another, so verification never matched); the
//!   port keeps the sounding behaviour, which is that a stored event verifies against its own body;
//! * an event's chain position is its place in the caller's append-only trail, one after the last;
//! * verification walks the trail in chain order and reports a mismatch per event, then folds the checksums into
//!   one chain hash by hashing the running hash with the next checksum;
//! * a single event's verification answers both checksums and whether they agree, and an event that is not the
//!   caller's is missing;
//! * a version snapshot is numbered one after the last snapshot the caller holds for that RESOURCE ID - the
//!   shipped count ignored the resource type, so two resource types with the same id share a numbering - and its
//!   checksum covers the state alone;
//! * lineage is derived from the caller's events for one resource id: the first `depth` of them become nodes and
//!   an edge is drawn from a correlation id to the event it names, when that correlation is one of the caller's
//!   events. A resource with no events answers no lineage rather than an empty graph;
//! * a compliance report and the analytics summary are both derived from the caller's events, filtered by period
//!   and by resource type or actor, and never stored;
//! * the planning, report, trail and compliance assessments are pure: they answer from the caller's figures and
//!   take their "today" from the request so a reckoning can be repeated.

use serde_json::{json, Map, Value};

use vimbai_common::hash::sha256_hex;
use vimbai_store::read::{field, refused, text, whole};
use vimbai_store::{BookScope, Graph, RecordKind, Records, StoreError};

/// The trail an event is appended to.
pub const AUDIT_EVENT: RecordKind = RecordKind::new("AuditEvent", "OWNS_EVENT");

/// A version of a resource at a moment.
pub const VERSION_SNAPSHOT: RecordKind = RecordKind::new("VersionSnapshot", "OWNS_SNAPSHOT");

/// The fields an event body carries, which is what its checksum covers.
const EVENT_FIELDS: &[&str] = &[
    "id",
    "event_type",
    "resource_type",
    "resource_id",
    "user_id",
    "user_email",
    "organization_id",
    "action_details",
    "previous_state",
    "new_state",
    "metadata",
    "timestamp",
    "ip_address",
    "user_agent",
    "session_id",
    "correlation_id",
];

/// The body of a record, as the checksum sees it: the known fields only, absent ones as null, keys in sorted
/// order because that is how this JSON library holds an object.
///
/// An event's `user_id` is the ACTOR, not the caller, so the store keeps it under `actor_id` - the caller's own
/// id is stamped from the scope and may not come from the payload. The body is put back together the way the
/// shipping service described it, so the checksum covers the same shape on the way in and on the way out.
fn body(record: &Value, fields: &[&str]) -> Value {
    let mut held = Map::new();
    for name in fields {
        let value = if *name == "user_id" {
            match record.get("actor_id") {
                Some(actor) => actor.clone(),
                None => record.get("user_id").cloned().unwrap_or(Value::Null),
            }
        } else {
            record.get(*name).cloned().unwrap_or(Value::Null)
        };
        held.insert((*name).to_string(), value);
    }
    Value::Object(held)
}

/// The checksum an event body earns.
pub fn event_checksum(record: &Value) -> String {
    sha256_hex(body(record, EVENT_FIELDS).to_string().as_bytes())
}

/// The checksum a version's state earns.
pub fn state_checksum(state: &Value) -> String {
    sha256_hex(state.to_string().as_bytes())
}

/// The actor an event names: stored under `actor_id`, because the caller's own id is stamped separately.
fn actor_of(record: &Value) -> &str {
    let held = field(record, "actor_id");
    if held.is_empty() {
        field(record, "user_id")
    } else {
        held
    }
}

fn position_of(record: &Value) -> u64 {
    record
        .get("chain_position")
        .and_then(|held| match held {
            Value::Number(number) => number.as_u64(),
            Value::String(digits) => digits.trim().parse().ok(),
            _ => None,
        })
        .unwrap_or(0)
}

fn timestamp_of(record: &Value) -> &str {
    field(record, "timestamp")
}

/// The caller's events in chain order.
fn chain(graph: &(impl Graph + ?Sized), scope: &BookScope) -> Result<Vec<Value>, StoreError> {
    let mut held = Records::new(graph, AUDIT_EVENT).list(scope, 500)?;
    held.sort_by(|one, other| {
        position_of(one)
            .cmp(&position_of(other))
            .then_with(|| timestamp_of(one).cmp(timestamp_of(other)))
    });
    Ok(held)
}

fn event_answer(record: &Value) -> Value {
    let mut held = body(record, EVENT_FIELDS);
    held["checksum"] = record.get("checksum").cloned().unwrap_or(Value::Null);
    held
}

/// `POST /events`: append an event to the trail.
pub fn create_event(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    if text(payload, "event_type").is_empty() {
        return Err(refused("event_type is required"));
    }
    if text(payload, "resource_type").is_empty() {
        return Err(refused("resource_type is required"));
    }
    let held = chain(graph, scope)?;
    let id = {
        let stated = text(payload, "id");
        if stated.is_empty() {
            vimbai_common::ids::service_id()
        } else {
            stated
        }
    };
    let mut fields = body(payload, EVENT_FIELDS);
    fields["id"] = json!(id);
    // the shipped service stamped the arrival time; the port takes the caller's, so a replay hashes the same
    let stamped = text(payload, "timestamp");
    fields["timestamp"] = json!(if stamped.is_empty() {
        text(payload, "now")
    } else {
        stamped
    });
    let checksum = event_checksum(&fields);

    let mut stored = fields.clone();
    // the actor travels under its own name, because the caller's id is stamped by the store
    let actor = stored
        .as_object_mut()
        .and_then(|object| object.remove("user_id"))
        .unwrap_or(Value::Null);
    stored["actor_id"] = actor;
    stored["checksum"] = json!(checksum);
    // append-only: the position is one after the last the caller holds
    stored["chain_position"] = json!(held.len() as u64);
    let created = Records::new(graph, AUDIT_EVENT).create(scope, &stored)?;
    Ok(event_answer(&created))
}

/// `GET /events`: the caller's events, newest first, narrowed by any filter given.
pub fn list_events(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let mut held = Records::new(graph, AUDIT_EVENT).list(scope, 500)?;
    for (key, wanted) in [
        ("resource_type", text(query, "resource_type")),
        ("resource_id", text(query, "resource_id")),
        ("event_type", text(query, "event_type")),
    ] {
        if !wanted.is_empty() {
            held.retain(|event| field(event, key) == wanted);
        }
    }
    let actor = text(query, "user_id");
    if !actor.is_empty() {
        held.retain(|event| actor_of(event) == actor);
    }
    held.sort_by(|one, other| timestamp_of(other).cmp(timestamp_of(one)));
    let total = held.len();
    let offset = query.get("offset").and_then(Value::as_u64).unwrap_or(0) as usize;
    let limit = match query.get("limit").and_then(Value::as_u64) {
        Some(stated) => stated as usize,
        None => 100,
    };
    let page: Vec<Value> = held
        .iter()
        .skip(offset)
        .take(limit)
        .map(event_answer)
        .collect();
    Ok(json!({"total": total, "events": page}))
}

/// `GET /events/{id}`: one event.
pub fn get_event(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    event_id: &str,
) -> Result<Value, StoreError> {
    let held = Records::new(graph, AUDIT_EVENT).get(scope, event_id)?;
    Ok(event_answer(&held))
}

/// `POST /versions/{resource_type}/{resource_id}`: keep a version of a resource.
pub fn create_version(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    resource_type: &str,
    resource_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    if resource_type.is_empty() {
        return Err(refused("resource_type is required"));
    }
    // the shipped count was by resource id alone, so two types sharing an id share a numbering
    let mine = Records::new(graph, VERSION_SNAPSHOT).list(scope, 500)?;
    let version = mine
        .iter()
        .filter(|held| field(held, "resource_id") == resource_id)
        .count()
        + 1;
    let state = payload.get("state").cloned().unwrap_or_else(|| json!({}));
    let changed_at = {
        let stated = text(payload, "changed_at");
        if stated.is_empty() {
            text(payload, "now")
        } else {
            stated
        }
    };
    let fields = json!({
        "id": vimbai_common::ids::service_id(),
        "resource_type": resource_type,
        "resource_id": resource_id,
        "version": version,
        "state": state,
        "changed_by": text(payload, "changed_by"),
        "changed_at": changed_at,
        "change_reason": payload.get("change_reason").cloned().unwrap_or(Value::Null),
        // the checksum covers the state alone, as shipped
        "checksum": state_checksum(&state),
    });
    let created = Records::new(graph, VERSION_SNAPSHOT).create(scope, &fields)?;
    Ok(created)
}

/// `GET /versions/{resource_type}/{resource_id}`: a resource's versions in order.
pub fn version_history(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    resource_type: &str,
    resource_id: &str,
) -> Result<Value, StoreError> {
    let held = Records::new(graph, VERSION_SNAPSHOT).list(scope, 500)?;
    let mut mine: Vec<Value> = held
        .into_iter()
        .filter(|snapshot| {
            field(snapshot, "resource_id") == resource_id
                && field(snapshot, "resource_type") == resource_type
        })
        .collect();
    mine.sort_by_key(|snapshot| whole(snapshot, "version"));
    Ok(json!({"resource_id": resource_id, "versions": mine}))
}

/// `GET /lineage/{resource_id}`: the lineage the caller's events trace for a resource.
pub fn lineage(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    resource_id: &str,
    query: &Value,
) -> Result<Value, StoreError> {
    let held = chain(graph, scope)?;
    let mine: Vec<&Value> = held
        .iter()
        .filter(|event| field(event, "resource_id") == resource_id)
        .collect();
    if mine.is_empty() {
        return Ok(json!({
            "resource_id": resource_id,
            "lineage": [],
            "message": "No lineage data found",
        }));
    }
    let depth = match query.get("depth").and_then(Value::as_u64) {
        Some(stated) => stated as usize,
        None => 10,
    };
    let known: Vec<&str> = held.iter().map(|event| field(event, "id")).collect();
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    for event in mine.iter().take(depth) {
        let event_id = field(event, "id");
        nodes.push(json!({
            "id": event_id,
            "resource_type": field(event, "resource_type"),
            "resource_id": field(event, "resource_id"),
            "operation": field(event, "event_type"),
            "timestamp": timestamp_of(event),
            "user_id": actor_of(event),
            "source_event_id": event.get("correlation_id").cloned().unwrap_or(Value::Null),
        }));
        let correlation = field(event, "correlation_id");
        if !correlation.is_empty() && known.contains(&correlation) {
            edges.push(json!({
                "id": vimbai_common::ids::service_id(),
                "from_node_id": correlation,
                "to_node_id": event_id,
                "relationship_type": "caused_by",
            }));
        }
    }
    Ok(json!({
        "resource_id": resource_id,
        "nodes": nodes,
        "edges": edges,
        "total_events": mine.len(),
        "displayed_events": nodes.len(),
    }))
}

/// `GET /integrity/verify`: the caller's whole trail, folded into one chain hash.
pub fn verify_chain(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let held = chain(graph, scope)?;
    let mut errors: Vec<Value> = Vec::new();
    let mut chain_hash = String::new();
    for event in &held {
        let stored = field(event, "checksum");
        if event_checksum(event) != stored {
            errors.push(json!(format!(
                "Checksum mismatch for event {}",
                field(event, "id")
            )));
        }
        chain_hash = sha256_hex(format!("{chain_hash}{stored}").as_bytes());
    }
    Ok(json!({
        "is_valid": errors.is_empty(),
        "total_events": held.len(),
        "chain_length": held.len(),
        "chain_hash": chain_hash,
        "errors": errors,
        "verified_at": text(query, "now"),
    }))
}

/// `GET /integrity/verify-event/{id}`: one event against its own checksum.
pub fn verify_event(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    event_id: &str,
    query: &Value,
) -> Result<Value, StoreError> {
    let held = Records::new(graph, AUDIT_EVENT).get(scope, event_id)?;
    let stored = field(&held, "checksum").to_string();
    let computed = event_checksum(&held);
    Ok(json!({
        "event_id": event_id,
        "stored_checksum": stored,
        "computed_checksum": computed,
        "matches": computed == stored,
        "verified_at": text(query, "now"),
    }))
}

// ---------------------------------------------------------------------------
// The assessments: pure, answered from the caller's own figures.
// ---------------------------------------------------------------------------

fn round(value: f64, places: i32) -> f64 {
    let factor = 10f64.powi(places);
    (value * factor).round() / factor
}

fn rows<'a>(payload: &'a Value, key: &str) -> Vec<&'a Value> {
    match payload.get(key) {
        Some(Value::Array(held)) => held.iter().collect(),
        _ => Vec::new(),
    }
}

fn stated_text(row: &Value, key: &str) -> String {
    row.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// `POST /planning/plan`: the plan an audit starts from.
pub fn plan_audit(payload: &Value) -> Value {
    let fiscal_year = payload
        .get("fiscal_year")
        .map(|held| match held {
            Value::String(digits) => digits.clone(),
            other => other.to_string(),
        })
        .unwrap_or_default();
    // the shipped plan worked from a fixed base rather than from the company's own figures
    let planning_materiality = 1_000_000.0 * 0.5;
    let performance_materiality = planning_materiality * 0.75;

    let mut high = 0;
    let mut medium = 0;
    for finding in rows(payload, "prior_year_findings") {
        match stated_text(finding, "risk_level").as_str() {
            "high" => high += 1,
            "low" => {}
            // a finding that names no level, or one that is not a level, is counted as medium
            _ => medium += 1,
        }
    }
    let overall_risk = if high > 3 {
        "high"
    } else if medium > 2 {
        "medium"
    } else {
        "low"
    };

    json!({
        "audit_id": stated_text(payload, "audit_id"),
        "overall_risk": overall_risk,
        "materiality": {
            "planning_materiality": round(planning_materiality, 2),
            "performance_materiality": round(performance_materiality, 2),
            "thresholds_unadjusted": round(performance_materiality * 0.05, 2),
        },
        "audit_scope": {
            "entities": ["Entity_1", "Entity_2", "Entity_3"],
            "periods": [fiscal_year],
            "accounts": ["Revenue", "Assets", "Liabilities", "Equity"],
            "locations": ["Head Office", "Regional Office 1"],
        },
        "risk_assessment": {
            "revenue_recognition": {
                "inherent_risk": "high",
                "control_risk": "medium",
                "detection_risk": 0.10,
                "risk_level": "high",
            },
            "inventory": {
                "inherent_risk": "medium",
                "control_risk": "low",
                "detection_risk": 0.05,
                "risk_level": "medium",
            },
        },
        "audit_strategy": {
            "approach": "Risk-based audit approach",
            "sampling_method": "Statistical sampling with random selection",
            "testing_strategy": "Substantive procedures for high-risk areas",
        },
        "resource_requirements": {
            "senior_auditors": 2,
            "junior_auditors": 4,
            "specialists": 1,
            "estimated_hours": 800,
        },
        "timeline": {
            "planning_start": format!("{fiscal_year}-01-01"),
            "fieldwork_start": format!("{fiscal_year}-03-01"),
            "fieldwork_end": format!("{fiscal_year}-05-31"),
            "report_issue": format!("{fiscal_year}-06-30"),
        },
        "key_focus_areas": [
            "Revenue recognition",
            "Going concern assessment",
            "Related party transactions",
        ],
    })
}

/// `POST /reports/generate`: a formal audit report.
pub fn generate_audit_report(payload: &Value) -> Value {
    let findings: Vec<Value> = rows(payload, "findings")
        .into_iter()
        .map(|finding| {
            let severity = stated_text(finding, "severity");
            json!({
                "finding_id": stated_text(finding, "id"),
                "description": stated_text(finding, "description"),
                "impact": stated_text(finding, "impact"),
                "severity": if severity.is_empty() { "medium".to_string() } else { severity },
                "recommendation": stated_text(finding, "recommendation"),
            })
        })
        .collect();

    let mut emphasis: Vec<String> = Vec::new();
    if !rows(payload, "material_weaknesses").is_empty() {
        emphasis.push("Material weaknesses in internal control".to_string());
    }
    if payload
        .get("going_concern_issues")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        emphasis.push("Substantial doubt about going concern".to_string());
    }
    if emphasis.is_empty() {
        emphasis.push("No emphasis of matter paragraphs".to_string());
    }

    let opinion = match stated_text(payload, "opinion").as_str() {
        "qualified" => "Qualified Opinion",
        "adverse" => "Adverse Opinion",
        "disclaimer" => "Disclaimer of Opinion",
        // anything else, including an unqualified opinion, and an opinion not named at all
        _ => "Unqualified Opinion",
    };

    json!({
        "audit_id": stated_text(payload, "audit_id"),
        "company_id": stated_text(payload, "company_id"),
        "fiscal_year": payload.get("fiscal_year").cloned().unwrap_or(Value::Null),
        // the shipped report took the day it ran; the port takes the caller's
        "report_date": text(payload, "today"),
        "opinion_type": opinion,
        "key_audit_matters": payload.get("key_audit_matters").cloned().unwrap_or_else(|| json!([])),
        "findings_summary": findings,
        "material_weaknesses_disclosure": !rows(payload, "material_weaknesses").is_empty(),
        "going_concern_disclosure": payload.get("going_concern_issues").cloned().unwrap_or(Value::Null),
        "emphasis_of_matter": emphasis,
        "regulatory_filing_required": ["SEC Filing", "Stock Exchange Filing", "Tax Authorities"],
    })
}

/// `POST /trails/analyze`: what a set of trail entries says about its actors.
pub fn analyse_trails(payload: &Value) -> Value {
    let entries = rows(payload, "entries");
    let mut counted: Vec<(String, u64)> = Vec::new();
    for entry in &entries {
        let actor = stated_text(entry, "user_id");
        match counted.iter_mut().find(|(name, _)| *name == actor) {
            Some((_, count)) => *count += 1,
            None => counted.push((actor, 1)),
        }
    }
    let activity: Vec<Value> = counted
        .iter()
        .map(|(actor, count)| json!({"user_id": actor, "actions": count}))
        .collect();
    let suspicious: Vec<Value> = counted
        .iter()
        .filter(|(_, count)| *count > 100)
        .map(|(actor, _)| json!({"user_id": actor, "reason": "High activity volume"}))
        .collect();
    let average = if counted.is_empty() {
        0.0
    } else {
        round(entries.len() as f64 / counted.len() as f64, 2)
    };
    json!({
        "company_id": stated_text(payload, "company_id"),
        "audit_summary": {
            "total_entries": entries.len(),
            "unique_users": counted.len(),
            "start_date": payload.get("start_date").cloned().unwrap_or(Value::Null),
            "end_date": payload.get("end_date").cloned().unwrap_or(Value::Null),
            "avg_actions_per_user": average,
        },
        "activity_by_user": activity,
        "suspicious_activities": suspicious,
    })
}

/// `POST /compliance/monitor`: the state of a set of compliance checks.
pub fn monitor_compliance(payload: &Value) -> Value {
    let checks = rows(payload, "checks");
    let counted = |wanted: &str| {
        checks
            .iter()
            .filter(|check| stated_text(check, "status") == wanted)
            .count()
    };
    let compliant = counted("Compliant");
    let rate = if checks.is_empty() {
        0.0
    } else {
        round(compliant as f64 / checks.len() as f64 * 100.0, 2)
    };
    let results: Vec<Value> = checks
        .iter()
        .map(|check| {
            json!({
                "check_id": stated_text(check, "check_id"),
                "regulation": stated_text(check, "regulation"),
                "status": stated_text(check, "status"),
                "last_checked": check.get("last_checked").cloned().unwrap_or(Value::Null),
            })
        })
        .collect();
    let critical: Vec<Value> = checks
        .iter()
        .filter(|check| stated_text(check, "status") == "Non-Compliant")
        .map(|check| json!(stated_text(check, "description")))
        .collect();
    json!({
        "company_id": stated_text(payload, "company_id"),
        "monitoring_date": text(payload, "now"),
        "compliance_summary": {
            "total_checks": checks.len(),
            "compliant": compliant,
            "non_compliant": counted("Non-Compliant"),
            "pending": counted("Pending"),
            "compliance_rate": rate,
        },
        "check_results": results,
        "critical_issues": critical,
    })
}

/// `POST /compliance/sox`: whether a set of controls was tested.
pub fn assess_sox(payload: &Value) -> Value {
    let controls = rows(payload, "controls");
    // a control counts as tested unless it says otherwise
    let passing = controls
        .iter()
        .filter(|control| {
            control
                .get("tested")
                .and_then(Value::as_bool)
                .unwrap_or(true)
        })
        .count();
    let failing = controls.len() - passing;
    json!({
        "company_id": stated_text(payload, "company_id"),
        "fiscal_year": payload.get("fiscal_year").cloned().unwrap_or(Value::Null),
        "overall_status": if failing == 0 { "compliant" } else { "needs_attention" },
        "control_count": controls.len(),
        "passing_controls": passing,
        "failing_controls": failing,
        "exceptions": [],
    })
}

/// `POST /compliance/gdpr`: the score a set of processing activities earns.
pub fn assess_gdpr(payload: &Value) -> Value {
    let consents = payload
        .get("consent_records")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let mut violations: Vec<String> = Vec::new();
    if consents < 1_000 {
        violations.push("Insufficient consent records".to_string());
    }
    if rows(payload, "retention_policies").len() < 3 {
        violations.push("Missing retention policies".to_string());
    }
    // the floor is a half: two violations still score a half rather than nothing
    let score = round((1.0 - violations.len() as f64 * 0.15).max(0.5), 4);
    json!({
        "company_id": stated_text(payload, "company_id"),
        "compliance_score": score,
        "data_subjects_count": consents * 2,
        "processing_activities": rows(payload, "data_processing_activities").len(),
        "violations": violations,
        "remediation_required": [
            "Update consent management system",
            "Implement data retention automation",
        ],
    })
}

// ---------------------------------------------------------------------------
// The two readings drawn from the trail itself.
// ---------------------------------------------------------------------------

fn within(event: &Value, start: &str, end: &str) -> bool {
    let stamped = timestamp_of(event);
    // a period end given as a bare date covers that whole day
    let ceiling = if end.len() == 10 {
        format!("{end}T23:59:59")
    } else {
        end.to_string()
    };
    (start.is_empty() || stamped >= start) && (end.is_empty() || stamped <= ceiling.as_str())
}

/// `POST /compliance/report`: a compliance report over the caller's events.
pub fn compliance_report(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let start = text(payload, "start_date");
    let end = text(payload, "end_date");
    let types: Vec<String> = match payload.get("resource_types") {
        Some(Value::Array(held)) => held
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect(),
        _ => Vec::new(),
    };
    let actors: Vec<String> = match payload.get("user_ids") {
        Some(Value::Array(held)) => held
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect(),
        _ => Vec::new(),
    };

    let held = Records::new(graph, AUDIT_EVENT).list(scope, 500)?;
    let mut events: Vec<Value> = held
        .into_iter()
        .filter(|event| within(event, &start, &end))
        .filter(|event| {
            types.is_empty()
                || types
                    .iter()
                    .any(|kind| field(event, "resource_type") == kind)
        })
        .filter(|event| actors.is_empty() || actors.iter().any(|actor| actor_of(event) == actor))
        .collect();
    events.sort_by(|one, other| timestamp_of(one).cmp(timestamp_of(other)));

    let mut by_type = std::collections::BTreeMap::new();
    let mut by_user = std::collections::BTreeMap::new();
    for event in &events {
        *by_type
            .entry(field(event, "event_type").to_string())
            .or_insert(0u64) += 1;
        *by_user.entry(actor_of(event).to_string()).or_insert(0u64) += 1;
    }

    let mut findings: Vec<Value> = Vec::new();
    let mut integrity_verified = true;
    if payload
        .get("include_integrity_checks")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        for event in chain(graph, scope)? {
            if event_checksum(&event) != field(&event, "checksum") {
                integrity_verified = false;
                findings.push(json!({
                    "type": "integrity_error",
                    "severity": "critical",
                    "description": format!("Checksum mismatch for event {}", field(&event, "id")),
                    "timestamp": text(payload, "now"),
                }));
            }
        }
    }
    for (actor, count) in &by_user {
        if *count > 1_000 {
            findings.push(json!({
                "type": "unusual_activity",
                "severity": "warning",
                "description": format!("User {actor} has {count} events in period"),
                "user_id": actor,
                "count": count,
            }));
        }
    }

    Ok(json!({
        "report_id": vimbai_common::ids::service_id(),
        "generated_at": text(payload, "now"),
        "period_start": start,
        "period_end": end,
        "total_events": events.len(),
        "events_by_type": by_type,
        "events_by_user": by_user,
        "integrity_verified": integrity_verified,
        "findings": findings,
    }))
}

/// `GET /analytics/summary`: what the caller's trail adds up to.
pub fn analytics(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let start = text(query, "start_date");
    let end = text(query, "end_date");
    let held = Records::new(graph, AUDIT_EVENT).list(scope, 500)?;
    let mut events: Vec<Value> = held
        .into_iter()
        .filter(|event| within(event, &start, &end))
        .collect();
    events.sort_by(|one, other| timestamp_of(one).cmp(timestamp_of(other)));

    let mut by_type = std::collections::BTreeMap::new();
    let mut by_resource = std::collections::BTreeMap::new();
    let mut by_user = std::collections::BTreeMap::new();
    for event in &events {
        *by_type
            .entry(field(event, "event_type").to_string())
            .or_insert(0u64) += 1;
        let resource = format!(
            "{}:{}",
            field(event, "resource_type"),
            field(event, "resource_id")
        );
        *by_resource.entry(resource).or_insert(0u64) += 1;
        *by_user.entry(actor_of(event).to_string()).or_insert(0u64) += 1;
    }
    let top = |counted: &std::collections::BTreeMap<String, u64>| -> Vec<Value> {
        let mut pairs: Vec<(String, u64)> = counted
            .iter()
            .map(|(name, count)| (name.clone(), *count))
            .collect();
        // the count decides, and the name breaks a tie so the answer does not wander
        pairs.sort_by(|one, other| other.1.cmp(&one.1).then_with(|| one.0.cmp(&other.0)));
        pairs
            .into_iter()
            .take(10)
            .map(|(name, count)| json!([name, count]))
            .collect()
    };

    Ok(json!({
        "total_events": events.len(),
        "by_event_type": by_type,
        "unique_resources": by_resource.len(),
        "unique_users": by_user.len(),
        "top_resources": top(&by_resource),
        "top_users": top(&by_user),
        "period_start": events.first().map(timestamp_of).unwrap_or(""),
        "period_end": events.last().map(timestamp_of).unwrap_or(""),
    }))
}

/// `GET /search`: events whose named fields hold the query.
pub fn search(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let wanted = text(query, "query").to_lowercase();
    if wanted.is_empty() {
        return Err(refused("query is required"));
    }
    let fields: Vec<String> = match query.get("fields") {
        Some(Value::Array(held)) if !held.is_empty() => held
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect(),
        _ => vec![
            "resource_id".to_string(),
            "user_id".to_string(),
            "action_details".to_string(),
        ],
    };
    let limit = match query.get("limit").and_then(Value::as_u64) {
        Some(stated) => stated as usize,
        None => 50,
    };
    let held = Records::new(graph, AUDIT_EVENT).list(scope, 500)?;
    let mut found: Vec<Value> = held
        .into_iter()
        .filter(|event| {
            fields.iter().any(|name| {
                if name == "user_id" {
                    // the actor is the one field held under its own name
                    return actor_of(event).to_lowercase().contains(&wanted);
                }
                match event.get(name.as_str()) {
                    // a field holding a structure is searched as the text it renders to
                    Some(value) if value.is_object() || value.is_array() => {
                        value.to_string().to_lowercase().contains(&wanted)
                    }
                    Some(Value::String(held)) => held.to_lowercase().contains(&wanted),
                    // a number or a flag is not text to search
                    _ => false,
                }
            })
        })
        .collect();
    found.sort_by(|one, other| timestamp_of(other).cmp(timestamp_of(one)));
    let total = found.len();
    found.truncate(limit);
    Ok(json!({
        "total": total,
        "events": found.iter().map(event_answer).collect::<Vec<Value>>(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::MemoryGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn an_event(
        graph: &MemoryGraph,
        scope: &BookScope,
        kind: &str,
        resource: &str,
        actor: &str,
    ) -> Value {
        create_event(
            graph,
            scope,
            &json!({
                "event_type": kind,
                "resource_type": "invoice",
                "resource_id": resource,
                "user_id": actor,
                "action_details": {"note": "posted"},
                "timestamp": "2026-03-01T10:00:00Z",
            }),
        )
        .expect("an event")
    }

    #[test]
    fn an_event_is_stamped_with_a_checksum_over_its_own_body() {
        let graph = MemoryGraph::new();
        let created = an_event(&graph, &scope(), "create", "inv-1", "user-1");
        assert!(!created["checksum"].as_str().expect("a checksum").is_empty());

        let verified =
            verify_event(&graph, &scope(), field(&created, "id"), &json!({})).expect("a check");
        assert_eq!(verified["matches"], json!(true));
        assert_eq!(verified["stored_checksum"], verified["computed_checksum"]);
    }

    #[test]
    fn a_trail_is_appended_in_order_and_verifies_as_a_whole() {
        let graph = MemoryGraph::new();
        for kind in ["create", "update", "delete"] {
            an_event(&graph, &scope(), kind, "inv-1", "user-1");
        }
        let described = verify_chain(&graph, &scope(), &json!({"now": "2026-03-02T09:00:00Z"}))
            .expect("a verification");
        assert_eq!(described["is_valid"], json!(true));
        assert_eq!(described["total_events"], json!(3));
        assert_eq!(described["chain_length"], json!(3));
        assert_eq!(described["errors"], json!([]));
        assert!(!described["chain_hash"]
            .as_str()
            .unwrap_or_default()
            .is_empty());
        assert_eq!(described["verified_at"], json!("2026-03-02T09:00:00Z"));
    }

    #[test]
    fn a_tampered_body_no_longer_verifies() {
        let graph = MemoryGraph::new();
        let created = an_event(&graph, &scope(), "create", "inv-1", "user-1");
        // rewrite the stored body behind the checksum's back
        Records::new(&graph, AUDIT_EVENT)
            .update(
                &scope(),
                field(&created, "id"),
                &json!({"resource_id": "inv-9"}),
            )
            .expect("a rewrite");
        let single =
            verify_event(&graph, &scope(), field(&created, "id"), &json!({})).expect("a check");
        assert_eq!(single["matches"], json!(false));
        let whole = verify_chain(&graph, &scope(), &json!({})).expect("a verification");
        assert_eq!(whole["is_valid"], json!(false));
        assert_eq!(whole["errors"].as_array().map(|held| held.len()), Some(1));
    }

    #[test]
    fn another_books_events_are_missing_rather_than_read() {
        let graph = MemoryGraph::new();
        let created = an_event(&graph, &scope(), "create", "inv-1", "user-1");
        let theirs = verify_event(
            &graph,
            &BookScope::new("user-2", "book-2"),
            field(&created, "id"),
            &json!({}),
        );
        assert!(theirs.is_err());
        let their_listing = list_events(&graph, &BookScope::new("user-2", "book-2"), &json!({}))
            .expect("a listing");
        assert_eq!(their_listing["total"], json!(0));
    }

    #[test]
    fn the_listing_narrows_and_pages() {
        let graph = MemoryGraph::new();
        an_event(&graph, &scope(), "create", "inv-1", "user-1");
        an_event(&graph, &scope(), "update", "inv-2", "user-9");
        let narrowed =
            list_events(&graph, &scope(), &json!({"event_type": "update"})).expect("a listing");
        assert_eq!(narrowed["total"], json!(1));
        assert_eq!(narrowed["events"][0]["resource_id"], json!("inv-2"));
        let paged =
            list_events(&graph, &scope(), &json!({"limit": 1, "offset": 1})).expect("a listing");
        assert_eq!(paged["total"], json!(2));
        assert_eq!(paged["events"].as_array().map(|held| held.len()), Some(1));
    }

    #[test]
    fn a_version_is_numbered_after_the_last_for_that_resource_id() {
        let graph = MemoryGraph::new();
        let first = create_version(
            &graph,
            &scope(),
            "invoice",
            "inv-1",
            &json!({"state": {"total": 100}, "changed_by": "user-1", "now": "2026-03-01T09:00:00Z"}),
        )
        .expect("a version");
        assert_eq!(first["version"], json!(1));
        let second = create_version(
            &graph,
            &scope(),
            "invoice",
            "inv-1",
            &json!({"state": {"total": 120}, "changed_by": "user-1", "now": "2026-03-02T09:00:00Z"}),
        )
        .expect("a version");
        assert_eq!(second["version"], json!(2));
        // the shipped count ignored the resource type, so a second type with the same id continues the numbering
        let other_type = create_version(
            &graph,
            &scope(),
            "credit_note",
            "inv-1",
            &json!({"state": {"total": 5}, "changed_by": "user-1"}),
        )
        .expect("a version");
        assert_eq!(other_type["version"], json!(3));

        let history = version_history(&graph, &scope(), "invoice", "inv-1").expect("a history");
        assert_eq!(
            history["versions"].as_array().map(|held| held.len()),
            Some(2)
        );
        assert_eq!(history["versions"][0]["version"], json!(1));
    }

    #[test]
    fn lineage_follows_a_correlation_and_says_when_there_is_none() {
        let graph = MemoryGraph::new();
        let first = an_event(&graph, &scope(), "create", "inv-1", "user-1");
        create_event(
            &graph,
            &scope(),
            &json!({
                "event_type": "update",
                "resource_type": "invoice",
                "resource_id": "inv-1",
                "user_id": "user-1",
                "correlation_id": field(&first, "id"),
                "timestamp": "2026-03-01T11:00:00Z",
            }),
        )
        .expect("an event");

        let traced = lineage(&graph, &scope(), "inv-1", &json!({})).expect("a lineage");
        assert_eq!(traced["total_events"], json!(2));
        assert_eq!(traced["displayed_events"], json!(2));
        assert_eq!(traced["edges"].as_array().map(|held| held.len()), Some(1));
        assert_eq!(traced["edges"][0]["relationship_type"], json!("caused_by"));

        let nothing = lineage(&graph, &scope(), "inv-404", &json!({})).expect("a lineage");
        assert_eq!(nothing["lineage"], json!([]));
        assert_eq!(nothing["message"], json!("No lineage data found"));
    }

    #[test]
    fn the_plan_works_from_the_shipped_base_and_the_prior_years_findings() {
        let quiet =
            plan_audit(&json!({"audit_id": "A-1", "fiscal_year": 2026, "prior_year_findings": []}));
        assert_eq!(quiet["overall_risk"], json!("low"));
        assert_eq!(
            quiet["materiality"]["planning_materiality"],
            json!(500000.0)
        );
        assert_eq!(
            quiet["materiality"]["performance_materiality"],
            json!(375000.0)
        );
        assert_eq!(
            quiet["materiality"]["thresholds_unadjusted"],
            json!(18750.0)
        );
        assert_eq!(quiet["timeline"]["fieldwork_end"], json!("2026-05-31"));

        // four high findings is what lifts the plan to high; three would not
        let loud = plan_audit(&json!({
            "audit_id": "A-1",
            "fiscal_year": 2026,
            "prior_year_findings": [
                {"risk_level": "high"}, {"risk_level": "high"},
                {"risk_level": "high"}, {"risk_level": "high"}
            ]
        }));
        assert_eq!(loud["overall_risk"], json!("high"));
        let middling = plan_audit(&json!({
            "audit_id": "A-1",
            "fiscal_year": 2026,
            "prior_year_findings": [{"risk_level": "medium"}, {"risk_level": "medium"}, {"risk_level": "medium"}]
        }));
        assert_eq!(middling["overall_risk"], json!("medium"));
    }

    #[test]
    fn the_report_names_its_opinion_and_its_emphasis() {
        let plain = generate_audit_report(&json!({
            "audit_id": "A-1",
            "company_id": "co-1",
            "fiscal_year": 2026,
            "opinion": "unqualified",
            "findings": [{"id": "F-1", "description": "Late filing"}],
            "today": "2026-06-30"
        }));
        assert_eq!(plain["opinion_type"], json!("Unqualified Opinion"));
        assert_eq!(plain["report_date"], json!("2026-06-30"));
        assert_eq!(plain["findings_summary"][0]["severity"], json!("medium"));
        assert_eq!(
            plain["emphasis_of_matter"],
            json!(["No emphasis of matter paragraphs"])
        );

        let hard = generate_audit_report(&json!({
            "audit_id": "A-1",
            "company_id": "co-1",
            "fiscal_year": 2026,
            "opinion": "adverse",
            "material_weaknesses": [{"id": "W-1"}],
            "going_concern_issues": true,
            "today": "2026-06-30"
        }));
        assert_eq!(hard["opinion_type"], json!("Adverse Opinion"));
        assert_eq!(hard["material_weaknesses_disclosure"], json!(true));
        assert_eq!(hard["material_weaknesses_disclosure"], json!(true));
        assert_eq!(
            hard["emphasis_of_matter"],
            json!([
                "Material weaknesses in internal control",
                "Substantial doubt about going concern"
            ])
        );
    }

    #[test]
    fn a_trail_is_analysed_for_its_actors() {
        let mut entries: Vec<Value> = (0..120).map(|_| json!({"user_id": "busy"})).collect();
        entries.extend((0..2).map(|_| json!({"user_id": "quiet"})));
        let analysed = analyse_trails(&json!({
            "company_id": "co-1",
            "entries": entries,
            "start_date": "2026-01-01",
            "end_date": "2026-03-31"
        }));
        assert_eq!(analysed["audit_summary"]["total_entries"], json!(122));
        assert_eq!(analysed["audit_summary"]["unique_users"], json!(2));
        assert_eq!(
            analysed["audit_summary"]["avg_actions_per_user"],
            json!(61.0)
        );
        // only the actor over a hundred actions is called out
        assert_eq!(
            analysed["suspicious_activities"]
                .as_array()
                .map(|h| h.len()),
            Some(1)
        );
        assert_eq!(
            analysed["suspicious_activities"][0]["user_id"],
            json!("busy")
        );
    }

    #[test]
    fn a_monitored_set_of_checks_answers_its_rate() {
        let monitored = monitor_compliance(&json!({
            "company_id": "co-1",
            "now": "2026-03-31T09:00:00Z",
            "checks": [
                {"check_id": "C-1", "regulation": "SOX", "status": "Compliant"},
                {"check_id": "C-2", "regulation": "GDPR", "status": "Non-Compliant", "description": "No consent log"},
                {"check_id": "C-3", "regulation": "IFRS", "status": "Pending"}
            ]
        }));
        assert_eq!(monitored["compliance_summary"]["compliant"], json!(1));
        assert_eq!(monitored["compliance_summary"]["non_compliant"], json!(1));
        assert_eq!(monitored["compliance_summary"]["pending"], json!(1));
        assert_eq!(
            monitored["compliance_summary"]["compliance_rate"],
            json!(33.33)
        );
        assert_eq!(monitored["critical_issues"], json!(["No consent log"]));

        // a company with no checks scores nothing rather than dividing by none
        let empty = monitor_compliance(&json!({"company_id": "co-1", "checks": []}));
        assert_eq!(empty["compliance_summary"]["compliance_rate"], json!(0.0));
    }

    #[test]
    fn sox_counts_untested_controls_as_passing() {
        let assessed = assess_sox(&json!({
            "company_id": "co-1",
            "fiscal_year": 2026,
            "controls": [{"id": "C-1"}, {"id": "C-2", "tested": true}, {"id": "C-3", "tested": false}]
        }));
        assert_eq!(assessed["passing_controls"], json!(2));
        assert_eq!(assessed["failing_controls"], json!(1));
        assert_eq!(assessed["overall_status"], json!("needs_attention"));
        assert_eq!(assessed["exceptions"], json!([]));
    }

    #[test]
    fn gdpr_scores_a_half_at_worst() {
        let poor = assess_gdpr(&json!({
            "company_id": "co-1",
            "consent_records": 10,
            "retention_policies": [],
            "data_processing_activities": [{"name": "payroll"}]
        }));
        assert_eq!(poor["violations"].as_array().map(|h| h.len()), Some(2));
        // two violations cost fifteen points each, and the half-point floor is only reached at four
        assert_eq!(poor["compliance_score"], json!(0.7));
        let worse = assess_gdpr(&json!({
            "company_id": "co-1",
            "consent_records": 10,
            "retention_policies": [],
            "extra_violations": 2
        }));
        assert_eq!(worse["compliance_score"], json!(0.7));
        assert_eq!(poor["data_subjects_count"], json!(20));
        assert_eq!(poor["processing_activities"], json!(1));

        let good = assess_gdpr(&json!({
            "company_id": "co-1",
            "consent_records": 5000,
            "retention_policies": [{"name": "a"}, {"name": "b"}, {"name": "c"}],
            "data_processing_activities": []
        }));
        assert_eq!(good["compliance_score"], json!(1.0));
        assert_eq!(good["violations"], json!([]));
    }

    #[test]
    fn the_compliance_report_is_derived_from_the_callers_events() {
        let graph = MemoryGraph::new();
        an_event(&graph, &scope(), "create", "inv-1", "user-1");
        an_event(&graph, &scope(), "update", "inv-1", "user-1");
        let reported = compliance_report(
            &graph,
            &scope(),
            &json!({
                "start_date": "2026-03-01",
                "end_date": "2026-03-31",
                "include_integrity_checks": true,
                "now": "2026-04-01T09:00:00Z"
            }),
        )
        .expect("a report");
        assert_eq!(reported["total_events"], json!(2));
        assert_eq!(reported["events_by_type"]["create"], json!(1));
        assert_eq!(reported["events_by_user"]["user-1"], json!(2));
        assert_eq!(reported["integrity_verified"], json!(true));
        assert_eq!(reported["findings"], json!([]));

        // a period that holds nothing still answers, with nothing in it
        let quiet = compliance_report(
            &graph,
            &scope(),
            &json!({"start_date": "2026-05-01", "end_date": "2026-05-31"}),
        )
        .expect("a report");
        assert_eq!(quiet["total_events"], json!(0));
        assert_eq!(quiet["integrity_verified"], json!(true));
    }

    #[test]
    fn the_summary_counts_types_resources_and_actors() {
        let graph = MemoryGraph::new();
        an_event(&graph, &scope(), "create", "inv-1", "user-1");
        an_event(&graph, &scope(), "create", "inv-1", "user-2");
        an_event(&graph, &scope(), "update", "inv-2", "user-1");
        let summary = analytics(&graph, &scope(), &json!({})).expect("a summary");
        assert_eq!(summary["total_events"], json!(3));
        assert_eq!(summary["unique_resources"], json!(2));
        assert_eq!(summary["unique_users"], json!(2));
        assert_eq!(summary["by_event_type"]["create"], json!(2));
        assert_eq!(summary["top_resources"][0], json!(["invoice:inv-1", 2]));
        assert_eq!(summary["period_start"], json!("2026-03-01T10:00:00Z"));
    }

    #[test]
    fn a_search_reads_the_text_it_is_given() {
        let graph = MemoryGraph::new();
        an_event(&graph, &scope(), "create", "inv-1", "user-1");
        let by_details = search(&graph, &scope(), &json!({"query": "POSTED"})).expect("a search");
        assert_eq!(by_details["total"], json!(1));
        let by_actor = search(
            &graph,
            &scope(),
            &json!({"query": "user-1", "fields": ["user_id"]}),
        )
        .expect("a search");
        assert_eq!(by_actor["total"], json!(1));
        let nothing = search(&graph, &scope(), &json!({"query": "absent"})).expect("a search");
        assert_eq!(nothing["total"], json!(0));
        // a search with no query is refused rather than answered with everything
        assert!(search(&graph, &scope(), &json!({"query": ""})).is_err());
    }
}
