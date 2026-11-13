#![forbid(unsafe_code)]
//! zero-trust-data-service: the access policies a Book sets, and every decision taken against them.
//!
//! Behaviours kept as shipped, each pinned by a test:
//!
//! * a policy is the caller's own, replaced whole when updated (the id it was asked for wins over the one sent), and
//!   deleting one that is not the caller's answers as missing rather than as forbidden;
//! * an evaluation finds the FIRST of the caller's policies naming the resource, so two policies for one resource
//!   leave the second unreachable - as they did when the shipped service scanned a list in order;
//! * a policy that requires no roles lets anyone through that gate, a required role is satisfied by holding ANY of
//!   them, and a required clearance is a floor rather than a match: a name the scale does not know counts as the
//!   lowest level, which is why a policy's default "standard" is the same gate as "public";
//! * MFA is only checked when the policy asks for it, and the address list only bites when it has something in it;
//! * every decision is recorded - granted or refused, with the reasons joined - and the subject of the decision is
//!   not the caller, so it is stored under its own name and presented as the user it was taken about.

use serde_json::{json, Value};

use vimbai_store::read::{field, flag, optional, or_default, refused, text};
use vimbai_store::{BookScope, Graph, RecordKind, Records, StoreError};

/// An access policy.
pub const POLICY: RecordKind = RecordKind::new("AccessPolicy", "OWNS_ACCESS_POLICY");

/// A decision taken against a policy.
pub const ATTEMPT: RecordKind = RecordKind::new("AccessAttempt", "OWNS_ACCESS_ATTEMPT");

/// The clearance scale, lowest first. A name it does not know is the lowest level.
pub const CLEARANCE_LEVELS: [(&str, u32); 4] = [
    ("public", 0),
    ("internal", 1),
    ("confidential", 2),
    ("restricted", 3),
];

fn level(name: &str) -> u32 {
    CLEARANCE_LEVELS
        .iter()
        .find(|(known, _)| *known == name)
        .map(|(_, rank)| *rank)
        .unwrap_or(0)
}

/// A list of words, given as a list or left empty.
fn words(payload: &Value, key: &str) -> Vec<String> {
    match payload.get(key) {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect(),
        Some(Value::String(one)) if !one.is_empty() => vec![one.clone()],
        _ => Vec::new(),
    }
}

fn policy_answer(record: &Value) -> Value {
    json!({
        "id": field(record, "id"),
        "resource": field(record, "resource"),
        "required_roles": words(record, "required_roles"),
        "required_clearance": text(record, "required_clearance"),
        "mfa_required": flag(record, "mfa_required"),
        "ip_whitelist": words(record, "ip_whitelist"),
        "created_at": field(record, "created_at"),
    })
}

fn attempt_answer(record: &Value) -> Value {
    json!({
        "id": field(record, "id"),
        "user_id": field(record, "subject_user_id"),
        "resource": field(record, "resource"),
        "policy_id": field(record, "policy_id"),
        "user_roles": words(record, "user_roles"),
        "user_clearance": text(record, "user_clearance"),
        "mfa_verified": flag(record, "mfa_verified"),
        "source_ip": field(record, "source_ip"),
        "granted": flag(record, "granted"),
        "timestamp": field(record, "timestamp"),
        "reason": field(record, "reason"),
    })
}

/// A policy as it is stored, from whatever the caller sent.
fn policy_fields(payload: &Value, id: &str) -> Value {
    let clearance = or_default(payload, "required_clearance", "standard");
    let mfa_required = match payload.get("mfa_required") {
        None | Some(Value::Null) => true,
        Some(_) => flag(payload, "mfa_required"),
    };
    json!({
        "id": id,
        "resource": text(payload, "resource"),
        "required_roles": words(payload, "required_roles"),
        "required_clearance": clearance,
        "mfa_required": mfa_required,
        "ip_whitelist": words(payload, "ip_whitelist"),
        "created_at": text(payload, "created_at"),
    })
}

/// `POST /policies`: set an access policy.
pub fn set_access_policy(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    if text(payload, "resource").is_empty() {
        return Err(refused("resource is required"));
    }
    let id = optional(payload, "id").unwrap_or_else(vimbai_common::ids::service_id);
    let created = Records::new(graph, POLICY).create(scope, &policy_fields(payload, &id))?;
    Ok(policy_answer(&created))
}

/// `GET /policies`: the caller's policies.
pub fn access_policies_held(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
) -> Result<Value, StoreError> {
    let held = Records::new(graph, POLICY).list(scope, 1000)?;
    Ok(Value::Array(held.iter().map(policy_answer).collect()))
}

/// `PUT /policies/{id}`: replace a policy whole. The id asked for wins over any id sent with it.
pub fn replace_policy(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    policy_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let records = Records::new(graph, POLICY);
    // another Book's policy, or one that was never set, simply answers as missing
    records.get(scope, policy_id)?;
    let replaced = records.update(scope, policy_id, &policy_fields(payload, policy_id))?;
    Ok(policy_answer(&replaced))
}

/// `DELETE /policies/{id}`.
pub fn delete_policy(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    policy_id: &str,
) -> Result<Value, StoreError> {
    Records::new(graph, POLICY).delete(scope, policy_id)?;
    Ok(json!({ "deleted": true, "policy_id": policy_id }))
}

/// `POST /evaluate`: decide an access request and keep the decision.
pub fn evaluate_access(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let subject = text(payload, "user_id");
    if subject.is_empty() {
        return Err(refused("user_id is required"));
    }
    let resource = text(payload, "resource");
    let roles = words(payload, "user_roles");
    let clearance = text(payload, "user_clearance");
    let mfa = flag(payload, "mfa_verified");
    let source_ip = text(payload, "source_ip");

    let held = Records::new(graph, POLICY).list(scope, 1000)?;
    // the OLDEST policy naming the resource is the one that answers: the shipped service took the first of a list it
    // scanned in order, and a stable rule is better than whatever order the graph happened to hand back. A second
    // policy for the same resource is therefore unreachable until the first is deleted, as it was before.
    let mut matching: Vec<&Value> = held
        .iter()
        .filter(|policy| field(policy, "resource") == resource)
        .collect();
    matching.sort_by_key(|policy| field(policy, "created_at").to_string());
    let policy = matching.first().copied();

    let (policy_id, granted, reason) = match policy {
        None => (
            String::new(),
            false,
            "No policy found for resource".to_string(),
        ),
        Some(policy) => {
            let required = words(policy, "required_roles");
            let has_role = required.is_empty() || required.iter().any(|role| roles.contains(role));
            // a floor, not a match: a name the scale does not know is the lowest level
            let has_clearance = level(&clearance) >= level(&text(policy, "required_clearance"));
            let mfa_ok = !flag(policy, "mfa_required") || mfa;
            let listed = words(policy, "ip_whitelist");
            let ip_ok = listed.is_empty() || listed.contains(&source_ip);

            let mut reasons: Vec<String> = Vec::new();
            if !has_role {
                reasons.push("Missing required role".to_string());
            }
            if !has_clearance {
                reasons.push("Insufficient clearance".to_string());
            }
            if !mfa_ok {
                reasons.push("MFA required but not verified".to_string());
            }
            if !ip_ok {
                reasons.push("IP not in whitelist".to_string());
            }
            let granted = reasons.is_empty();
            let reason = if granted {
                "Access granted".to_string()
            } else {
                reasons.join("; ")
            };
            (field(policy, "id").to_string(), granted, reason)
        }
    };

    let fields = json!({
        "id": vimbai_common::ids::service_id(),
        "subject_user_id": subject,
        "resource": resource,
        "policy_id": policy_id,
        "user_roles": roles,
        "user_clearance": clearance,
        "mfa_verified": mfa,
        "source_ip": source_ip,
        "granted": granted,
        "timestamp": text(payload, "timestamp"),
        "reason": reason,
    });
    let created = Records::new(graph, ATTEMPT).create(scope, &fields)?;
    Ok(attempt_answer(&created))
}

/// `GET /attempts`: the caller's recent decisions, oldest first, the last `limit` of them.
pub fn list_attempts(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let limit = limit_of(payload);
    let mut held = Records::new(graph, ATTEMPT).list(scope, 1000)?;
    held.sort_by_key(|attempt| field(attempt, "timestamp").to_string());
    if held.len() > limit {
        held.drain(0..held.len() - limit);
    }
    Ok(Value::Array(held.iter().map(attempt_answer).collect()))
}

/// `GET /attempts/user/{id}`: the caller's decisions about one subject.
pub fn attempts_for_user(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    subject: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let limit = limit_of(payload);
    let mut held: Vec<Value> = Records::new(graph, ATTEMPT)
        .list(scope, 1000)?
        .into_iter()
        .filter(|attempt| field(attempt, "subject_user_id") == subject)
        .collect();
    held.sort_by_key(|attempt| field(attempt, "timestamp").to_string());
    if held.len() > limit {
        held.drain(0..held.len() - limit);
    }
    Ok(Value::Array(held.iter().map(attempt_answer).collect()))
}

/// The tail asked for: fifty when nothing was said, and never nothing at all.
fn limit_of(payload: &Value) -> usize {
    let asked = vimbai_store::read::whole(payload, "limit") as usize;
    if asked == 0 {
        50
    } else {
        asked
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::MemoryGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn a_policy(graph: &MemoryGraph, payload: Value) -> Value {
        set_access_policy(graph, &scope(), &payload).expect("a policy")
    }

    #[test]
    fn a_resource_with_no_policy_is_refused_and_the_decision_kept() {
        let graph = MemoryGraph::new();
        let decided = evaluate_access(
            &graph,
            &scope(),
            &json!({"user_id": "person-1", "resource": "ledger", "timestamp": "2026-01-01T09:00:00Z"}),
        )
        .expect("a decision");
        assert_eq!(decided["granted"], json!(false));
        assert_eq!(decided["reason"], json!("No policy found for resource"));
        assert_eq!(decided["policy_id"], json!(""));
        // the subject of the decision is not the caller
        assert_eq!(decided["user_id"], json!("person-1"));
        let kept = list_attempts(&graph, &scope(), &json!({})).expect("a listing");
        assert_eq!(kept.as_array().map(|held| held.len()), Some(1));
    }

    #[test]
    fn a_required_role_is_satisfied_by_any_of_them_and_clearance_is_a_floor() {
        let graph = MemoryGraph::new();
        let policy = a_policy(
            &graph,
            json!({
                "resource": "ledger",
                "required_roles": ["clerk", "auditor"],
                "required_clearance": "internal",
                "mfa_required": false,
                "created_at": "2026-01-01T09:00:00Z"
            }),
        );
        let allowed = evaluate_access(
            &graph,
            &scope(),
            &json!({
                "user_id": "person-1",
                "resource": "ledger",
                "user_roles": ["auditor"],
                "user_clearance": "restricted",
                "timestamp": "2026-01-02T09:00:00Z"
            }),
        )
        .expect("a decision");
        assert_eq!(allowed["granted"], json!(true));
        assert_eq!(allowed["reason"], json!("Access granted"));
        assert_eq!(allowed["policy_id"], policy["id"]);

        let refused_decision = evaluate_access(
            &graph,
            &scope(),
            &json!({
                "user_id": "person-2",
                "resource": "ledger",
                "user_roles": ["clerk"],
                "user_clearance": "public",
                "timestamp": "2026-01-03T09:00:00Z"
            }),
        )
        .expect("a decision");
        assert_eq!(refused_decision["granted"], json!(false));
        assert_eq!(refused_decision["reason"], json!("Insufficient clearance"));
    }

    #[test]
    fn a_policy_that_asks_for_nothing_lets_anyone_through() {
        let graph = MemoryGraph::new();
        a_policy(
            &graph,
            json!({"resource": "catalogue", "mfa_required": false, "created_at": "2026-01-01T09:00:00Z"}),
        );
        let decided = evaluate_access(
            &graph,
            &scope(),
            &json!({"user_id": "person-1", "resource": "catalogue", "timestamp": "2026-01-02T09:00:00Z"}),
        )
        .expect("a decision");
        // no roles required, and a policy's default clearance reads as the lowest level
        assert_eq!(decided["granted"], json!(true));
    }

    #[test]
    fn mfa_and_the_address_list_bite_only_when_the_policy_asks() {
        let graph = MemoryGraph::new();
        a_policy(
            &graph,
            json!({
                "resource": "payroll",
                "mfa_required": true,
                "ip_whitelist": ["10.0.0.1"],
                "created_at": "2026-01-01T09:00:00Z"
            }),
        );
        let refused_decision = evaluate_access(
            &graph,
            &scope(),
            &json!({
                "user_id": "person-1",
                "resource": "payroll",
                "source_ip": "10.0.0.9",
                "timestamp": "2026-01-02T09:00:00Z"
            }),
        )
        .expect("a decision");
        assert_eq!(
            refused_decision["reason"],
            json!("MFA required but not verified; IP not in whitelist")
        );
        let allowed = evaluate_access(
            &graph,
            &scope(),
            &json!({
                "user_id": "person-1",
                "resource": "payroll",
                "mfa_verified": true,
                "source_ip": "10.0.0.1",
                "timestamp": "2026-01-03T09:00:00Z"
            }),
        )
        .expect("a decision");
        assert_eq!(allowed["granted"], json!(true));
    }

    #[test]
    fn the_first_policy_naming_a_resource_is_the_one_that_answers() {
        let graph = MemoryGraph::new();
        a_policy(
            &graph,
            json!({"resource": "ledger", "required_roles": ["clerk"], "mfa_required": false,
                   "created_at": "2026-01-01T09:00:00Z"}),
        );
        let second = a_policy(
            &graph,
            json!({"resource": "ledger", "required_roles": ["nobody"], "mfa_required": false,
                   "created_at": "2026-01-02T09:00:00Z"}),
        );
        let decided = evaluate_access(
            &graph,
            &scope(),
            &json!({"user_id": "person-1", "resource": "ledger", "user_roles": ["clerk"],
                    "timestamp": "2026-01-03T09:00:00Z"}),
        )
        .expect("a decision");
        // the second policy is unreachable, as it was when the shipped service scanned a list in order
        assert_eq!(decided["granted"], json!(true));
        assert_ne!(decided["policy_id"], second["id"]);
    }

    #[test]
    fn a_policy_is_replaced_whole_and_only_by_the_book_that_set_it() {
        let graph = MemoryGraph::new();
        let policy = a_policy(
            &graph,
            json!({"resource": "ledger", "required_roles": ["clerk"], "created_at": "2026-01-01T09:00:00Z"}),
        );
        let id = field(&policy, "id").to_string();
        let replaced = replace_policy(
            &graph,
            &scope(),
            &id,
            &json!({"id": "not-this-one", "resource": "payroll", "mfa_required": false}),
        )
        .expect("a replacement");
        assert_eq!(replaced["id"], json!(id));
        assert_eq!(replaced["resource"], json!("payroll"));
        // the roles that were there are gone: a replacement is whole
        assert_eq!(replaced["required_roles"], json!([]));

        assert!(replace_policy(
            &graph,
            &BookScope::new("user-2", "book-2"),
            &id,
            &json!({"resource": "payroll"})
        )
        .is_err());
        assert!(
            delete_policy(&graph, &BookScope::new("user-2", "book-2"), &id).is_err(),
            "another Book's policy answers as missing"
        );
        delete_policy(&graph, &scope(), &id).expect("a deletion");
        let left = access_policies_held(&graph, &scope()).expect("a listing");
        assert_eq!(left.as_array().map(|held| held.len()), Some(0));
    }

    #[test]
    fn decisions_are_listed_oldest_first_and_the_tail_is_what_is_asked_for() {
        let graph = MemoryGraph::new();
        for (index, stamp) in [
            "2026-01-03T09:00:00Z",
            "2026-01-01T09:00:00Z",
            "2026-01-02T09:00:00Z",
        ]
        .iter()
        .enumerate()
        {
            evaluate_access(
                &graph,
                &scope(),
                &json!({"user_id": format!("person-{index}"), "resource": "ledger", "timestamp": stamp}),
            )
            .expect("a decision");
        }
        let kept = list_attempts(&graph, &scope(), &json!({"limit": 2})).expect("a listing");
        assert_eq!(kept.as_array().map(|held| held.len()), Some(2));
        // the two latest
        assert_eq!(kept[0]["user_id"], json!("person-2"));
        assert_eq!(kept[1]["user_id"], json!("person-0"));

        let theirs =
            attempts_for_user(&graph, &scope(), "person-1", &json!({})).expect("a listing");
        assert_eq!(theirs.as_array().map(|held| held.len()), Some(1));
    }
}
