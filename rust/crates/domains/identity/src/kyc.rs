#![forbid(unsafe_code)]
//! financial-identity-service: the KYC profile a Book keeps for a person, and what verification did to it.
//!
//! Behaviours kept as shipped, each pinned by a test:
//!
//! * a profile names a SUBJECT - the person it describes - which is not the caller. The caller owns the profile and
//!   the subject is recorded alongside it, so a Book can hold profiles for people who never sign in;
//! * a profile arrives pending, with no documents and a risk of nothing;
//! * a verification with two documents or more marks the profile verified, stamps the moment and scores it twenty -
//!   low risk. Fewer than two documents leaves the STATE where it was (it does not fail it) but scores the profile
//!   eighty, high risk, and the documents that were handed over are kept either way;
//! * a profile asked for by id, or by subject, is the caller's own or missing. By subject, the earliest profile the
//!   caller holds for that person answers - so a re-registration does not displace the first answer.

use serde_json::{json, Value};

use vimbai_store::read::{field, refused, text};
use vimbai_store::{BookScope, Graph, RecordKind, Records, StoreError};

/// A person's financial identity, as one Book knows it.
pub const PROFILE: RecordKind = RecordKind::new("FinancialProfile", "OWNS_PROFILE");

/// Documents a verification needs before it will call a profile verified.
pub const DOCUMENTS_TO_VERIFY: usize = 2;

/// The risk a verified profile carries.
pub const RISK_VERIFIED: i64 = 20;

/// The risk a profile carries when verification was not satisfied.
pub const RISK_UNVERIFIED: i64 = 80;

fn answer(record: &Value) -> Value {
    let state = {
        let held = field(record, "verification_status");
        if held.is_empty() {
            "pending".to_string()
        } else {
            held.to_string()
        }
    };
    json!({
        "id": field(record, "id"),
        "user_id": field(record, "subject_id"),
        "legal_name": field(record, "legal_name"),
        "national_id": field(record, "national_id"),
        "tax_id": field(record, "tax_id"),
        "date_of_birth": nullable(record, "date_of_birth"),
        "address": field(record, "address"),
        "phone": field(record, "phone"),
        "email": field(record, "email"),
        "employer": field(record, "employer"),
        "annual_income": number(record, "annual_income"),
        "verification_status": state,
        "verified_at": nullable(record, "verified_at"),
        "risk_score": number(record, "risk_score").as_i64().unwrap_or(0),
        "kyc_documents": documents(record),
    })
}

fn nullable(record: &Value, name: &str) -> Value {
    let held = field(record, name);
    if held.is_empty() {
        Value::Null
    } else {
        json!(held)
    }
}

fn number(record: &Value, name: &str) -> Value {
    match record.get(name) {
        Some(Value::Number(held)) => json!(held),
        Some(Value::String(held)) => held
            .parse::<f64>()
            .map(|parsed| json!(parsed))
            .unwrap_or_else(|_| json!(0)),
        _ => json!(0),
    }
}

fn documents(record: &Value) -> Vec<String> {
    match record.get("kyc_documents") {
        Some(Value::Array(held)) => held
            .iter()
            .filter_map(|one| one.as_str().map(str::to_string))
            .collect(),
        _ => Vec::new(),
    }
}

/// `POST /profiles`: record a person's financial identity.
pub fn create_profile(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let subject = text(payload, "user_id");
    if subject.is_empty() {
        return Err(refused(
            "user_id is required for the subject the profile describes",
        ));
    }
    if text(payload, "legal_name").is_empty() {
        return Err(refused("legal_name is required"));
    }
    let fields = json!({
        "id": vimbai_common::ids::service_id(),
        "subject_id": subject,
        "legal_name": text(payload, "legal_name"),
        "national_id": text(payload, "national_id"),
        "tax_id": text(payload, "tax_id"),
        "date_of_birth": text(payload, "date_of_birth"),
        "address": text(payload, "address"),
        "phone": text(payload, "phone"),
        "email": text(payload, "email"),
        "employer": text(payload, "employer"),
        "annual_income": number(payload, "annual_income"),
        "verification_status": "pending",
        "verified_at": "",
        "risk_score": 0,
        "kyc_documents": json!([]),
        "created_at": text(payload, "created_at"),
    });
    let created = Records::new(graph, PROFILE).create(scope, &fields)?;
    Ok(answer(&created))
}

/// `GET /profiles/{id}`: one profile, if it is the caller's.
pub fn get_profile(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    profile_id: &str,
) -> Result<Value, StoreError> {
    let record = Records::new(graph, PROFILE).get(scope, profile_id)?;
    Ok(answer(&record))
}

/// `PUT /profiles/{id}/verify`: note the documents handed over and what they earned.
///
/// The documents arrive as repeated query parameters (`?documents=a&documents=b`), which is how the shipped
/// endpoint took them, so the reader accepts either a list or the last value of one.
pub fn verify_profile(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    profile_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let found = Records::new(graph, PROFILE).get(scope, profile_id)?;
    let handed: Vec<String> = match payload.get("documents") {
        Some(Value::Array(held)) => held
            .iter()
            .filter_map(|one| one.as_str().map(str::to_string))
            .collect(),
        Some(Value::String(one)) if !one.is_empty() => vec![one.clone()],
        _ => Vec::new(),
    };
    let satisfied = handed.len() >= DOCUMENTS_TO_VERIFY;
    // the state only moves on a satisfied check; a failed one is scored, not marked
    let state = if satisfied {
        "verified".to_string()
    } else {
        let held = field(&found, "verification_status");
        if held.is_empty() {
            "pending".to_string()
        } else {
            held.to_string()
        }
    };
    let verified_at = if satisfied {
        text(payload, "verified_at")
    } else {
        field(&found, "verified_at").to_string()
    };
    let risk = if satisfied {
        RISK_VERIFIED
    } else {
        RISK_UNVERIFIED
    };
    Records::new(graph, PROFILE).update(
        scope,
        profile_id,
        &json!({
            "kyc_documents": handed,
            "verification_status": state,
            "verified_at": verified_at,
            "risk_score": risk,
        }),
    )?;
    Ok(json!({
        "id": profile_id,
        "status": state,
        "risk_score": risk,
    }))
}

/// `GET /profiles/by-subject/{subject}`: the earliest profile this Book holds for a person.
pub fn profile_for_subject(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    subject_id: &str,
) -> Result<Value, StoreError> {
    let held = Records::new(graph, PROFILE).list(scope, 500)?;
    let mut mine: Vec<Value> = held
        .into_iter()
        .filter(|profile| field(profile, "subject_id") == subject_id)
        .collect();
    // the earliest profile answers, so a later registration does not displace it
    mine.sort_by_key(|profile| field(profile, "created_at").to_string());
    mine.into_iter()
        .next()
        .map(|profile| answer(&profile))
        .ok_or(StoreError::NotFound)
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::MemoryGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn a_profile(graph: &MemoryGraph, subject: &str, created: &str) -> Value {
        create_profile(
            graph,
            &scope(),
            &json!({
                "user_id": subject,
                "legal_name": "Tariro Moyo",
                "national_id": "63-123456-A-00",
                "annual_income": 42000.0,
                "date_of_birth": "1990-04-02T00:00:00Z",
                "created_at": created,
            }),
        )
        .expect("a profile")
    }

    #[test]
    fn a_profile_names_its_subject_and_arrives_unverified() {
        let graph = MemoryGraph::new();
        let made = a_profile(&graph, "user-9", "2026-03-01T09:00:00Z");
        // the subject is the person, not the caller who recorded them
        assert_eq!(made["user_id"], json!("user-9"));
        assert_eq!(made["verification_status"], json!("pending"));
        assert_eq!(made["risk_score"], json!(0));
        assert_eq!(made["kyc_documents"], json!([]));
        assert_eq!(made["annual_income"], json!(42000.0));
        assert_eq!(made["verified_at"], Value::Null);
    }

    #[test]
    fn two_documents_verify_and_score_low() {
        let graph = MemoryGraph::new();
        let made = a_profile(&graph, "user-9", "2026-03-01T09:00:00Z");
        let id = field(&made, "id").to_string();
        let checked = verify_profile(
            &graph,
            &scope(),
            &id,
            &json!({"documents": ["passport", "utility bill"], "verified_at": "2026-03-02T10:00:00Z"}),
        )
        .expect("a verification");
        assert_eq!(checked["status"], json!("verified"));
        assert_eq!(checked["risk_score"], json!(20));

        let after = get_profile(&graph, &scope(), &id).expect("a profile");
        assert_eq!(after["verification_status"], json!("verified"));
        assert_eq!(after["verified_at"], json!("2026-03-02T10:00:00Z"));
        assert_eq!(after["kyc_documents"], json!(["passport", "utility bill"]));
    }

    #[test]
    fn one_document_scores_high_but_does_not_fail_the_profile() {
        let graph = MemoryGraph::new();
        let made = a_profile(&graph, "user-9", "2026-03-01T09:00:00Z");
        let id = field(&made, "id").to_string();
        let checked = verify_profile(&graph, &scope(), &id, &json!({"documents": ["passport"]}))
            .expect("a verification");
        // the state stays as it was: an unsatisfied check is scored, not marked
        assert_eq!(checked["status"], json!("pending"));
        assert_eq!(checked["risk_score"], json!(80));

        let after = get_profile(&graph, &scope(), &id).expect("a profile");
        assert_eq!(after["verification_status"], json!("pending"));
        assert_eq!(after["kyc_documents"], json!(["passport"]));
    }

    #[test]
    fn the_earliest_profile_for_a_subject_answers() {
        let graph = MemoryGraph::new();
        let first = a_profile(&graph, "user-9", "2026-01-01T09:00:00Z");
        a_profile(&graph, "user-9", "2026-06-01T09:00:00Z");
        let found = profile_for_subject(&graph, &scope(), "user-9").expect("a profile");
        assert_eq!(found["id"], first["id"]);
        assert!(profile_for_subject(&graph, &scope(), "user-404").is_err());
    }

    #[test]
    fn another_books_profile_is_missing() {
        let graph = MemoryGraph::new();
        let made = a_profile(&graph, "user-9", "2026-03-01T09:00:00Z");
        let theirs = get_profile(
            &graph,
            &BookScope::new("user-2", "book-2"),
            field(&made, "id"),
        );
        assert!(theirs.is_err());
    }
}
