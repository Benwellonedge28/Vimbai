#![forbid(unsafe_code)]
//! mfa-auth-service: the second factor a person carries.
//!
//! This sits beside single sign-on because both answer the same question - is this really them - and neither is a
//! business domain. What it holds is the enrolment (a shared secret and a set of backup codes) and the short-lived
//! challenges raised to prove possession.
//!
//! Behaviours kept as shipped, each pinned by a test:
//!
//! * MFA belongs to the PERSON across every Book, so it is self-service only: asking to enrol or verify someone else is
//!   refused (403) rather than quietly ignored;
//! * enrolling again replaces the previous secret and codes rather than holding two, and answers the secret, the
//!   otpauth address a client renders as a QR code, and eight backup codes;
//! * verifying someone who never enrolled answers that MFA is not set up rather than failing an unknown code;
//! * SHIPPED QUIRK PINNED: the shipped check accepted any six digits and was marked "simplified" in its own comment.
//!   The port keeps that answer rather than silently tightening it into a real TOTP check that would reject codes the
//!   service used to accept. The comment here says so, so nobody mistakes it for verification;
//! * a challenge is raised for the caller alone, lasts five minutes, and is spent when it verifies. A wrong code does
//!   NOT spend it (the shipped service left it in place, so a fat-fingered code could be retried), an expired one is
//!   dropped and answers expired, and another person's challenge is missing rather than refused.
//!
//! The clock comes from the request, so expiry is exact rather than "about five minutes".

use serde_json::{json, Value};

use vimbai_common::hash::sha256_hex;
use vimbai_store::read::{field, optional, refused, text, whole};
use vimbai_store::{BookScope, Graph, RecordKind, Records, StoreError};

/// A person's second factor: one secret and a set of backup codes.
pub const ENROLMENT: RecordKind = RecordKind::new("MfaEnrolment", "OWNS_MFA_ENROLMENT");

/// A short-lived challenge, raised to prove possession.
pub const CHALLENGE: RecordKind = RecordKind::new("MfaChallenge", "OWNS_MFA_CHALLENGE");

/// How long a challenge lasts.
pub const CHALLENGE_MINUTES: i64 = 5;

/// How many backup codes an enrolment carries.
pub const BACKUP_CODES: usize = 8;

/// The alphabet a base32 secret is written in.
const BASE32: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

/// Twenty random-looking bytes as base32, the shape an authenticator app expects.
fn hidden(seed: &str) -> String {
    let bytes = sha256_hex(seed.as_bytes());
    let mut taken = Vec::with_capacity(20);
    for index in (0..40).step_by(2) {
        taken.push(u8::from_str_radix(&bytes[index..index + 2], 16).unwrap_or(0));
    }
    let mut out = String::new();
    let mut buffer = 0u32;
    let mut bits = 0u32;
    for byte in taken {
        buffer = (buffer << 8) | byte as u32;
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            let digit = ((buffer >> bits) & 0x1f) as usize;
            out.push(BASE32[digit] as char);
        }
    }
    if bits > 0 {
        let digit = ((buffer << (5 - bits)) & 0x1f) as usize;
        out.push(BASE32[digit] as char);
    }
    out
}

/// Eight backup codes, each eight characters of upper-case hex.
fn backup_codes(seed: &str) -> Vec<String> {
    (0..BACKUP_CODES)
        .map(|index| sha256_hex(format!("{seed}:backup:{index}").as_bytes())[..8].to_uppercase())
        .collect()
}

/// A six-digit code, as the shipped service minted it.
fn six_digits(seed: &str) -> String {
    let hex = sha256_hex(seed.as_bytes());
    let taken = u32::from_str_radix(&hex[..8], 16).unwrap_or(0);
    format!("{:06}", 100_000 + (taken % 900_000))
}

/// The shipped code check: six digits and nothing else. See the module note - this is not real TOTP verification.
fn accepted(_secret: &str, code: &str) -> bool {
    code.len() == 6 && code.chars().all(|digit| digit.is_ascii_digit())
}

fn enrolment_of(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
) -> Result<Option<Value>, StoreError> {
    let held = Records::new(graph, ENROLMENT).list(scope, 10)?;
    Ok(held.into_iter().next())
}

/// `POST /setup`: enrol the caller, replacing whatever was there.
pub fn setup(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let asked_for = text(payload, "user_id");
    // MFA is self-service: nobody enrols a second factor on another person's behalf
    if !asked_for.is_empty() && asked_for != scope.user_id {
        return Err(StoreError::Forbidden(
            "MFA operations are self-service only".to_string(),
        ));
    }
    let seed = format!("{}:{}", vimbai_common::ids::service_id(), scope.user_id);
    let secret = hidden(&seed);
    let codes = backup_codes(&seed);
    let records = Records::new(graph, ENROLMENT);
    // the person is stamped on the record from the scope, so it is not written again here
    let fields = json!({
        "secret": secret,
        "backup_codes": codes,
        "enrolled_at": text(payload, "now"),
    });
    match enrolment_of(graph, scope)? {
        Some(held) => {
            records.update(scope, field(&held, "id"), &fields)?;
        }
        None => {
            let mut fields = fields;
            fields["id"] = json!(vimbai_common::ids::service_id());
            records.create(scope, &fields)?;
        }
    }
    Ok(json!({
        "user_id": scope.user_id,
        "secret": secret,
        "qr_uri": format!("otpauth://totp/Vimbai:{}?secret={}&issuer=Vimbai", scope.user_id, secret),
        "backup_codes": codes,
    }))
}

/// `POST /verify`: check the caller's code against their own enrolment.
pub fn verify(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let asked_for = text(payload, "user_id");
    if !asked_for.is_empty() && asked_for != scope.user_id {
        return Err(StoreError::Forbidden(
            "MFA operations are self-service only".to_string(),
        ));
    }
    let code = text(payload, "code");
    let Some(held) = enrolment_of(graph, scope)? else {
        return Ok(json!({
            "verified": false,
            "user_id": scope.user_id,
            "message": "MFA not set up for this user",
        }));
    };
    if accepted(&text(&held, "secret"), &code) {
        // the shipped service handed back a token shaped like this, and nothing in the port relies on it
        let token = sha256_hex(format!("{}{}", scope.user_id, text(payload, "now")).as_bytes());
        return Ok(json!({
            "verified": true,
            "user_id": scope.user_id,
            "message": "MFA verification successful",
            "access_token": token,
        }));
    }
    Ok(json!({
        "verified": false,
        "user_id": scope.user_id,
        "message": "Invalid MFA code",
    }))
}

/// `POST /challenge`: raise a five-minute challenge for the caller.
pub fn raise_challenge(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let asked_for = match payload.get("user_id").and_then(Value::as_str) {
        Some(given) if !given.is_empty() => given.to_string(),
        _ => scope.user_id.clone(),
    };
    if asked_for != scope.user_id {
        return Err(StoreError::Forbidden(
            "MFA operations are self-service only".to_string(),
        ));
    }
    let method = match text(payload, "method") {
        empty if empty.is_empty() => "totp".to_string(),
        given => given,
    };
    let seed = format!(
        "{}:{}:challenge",
        vimbai_common::ids::service_id(),
        scope.user_id
    );
    let challenge_id = vimbai_common::ids::service_id().replace('-', "");
    Records::new(graph, CHALLENGE).create(
        scope,
        &json!({
            "id": vimbai_common::ids::service_id(),
            "challenge_id": challenge_id,
            "code": six_digits(&seed),
            "method": method,
            "expires_at": text(payload, "now_plus_five"),
            "raised_at": text(payload, "now"),
        }),
    )?;
    Ok(json!({
        "challenge_id": challenge_id,
        "method": method,
        "expires_in_minutes": CHALLENGE_MINUTES,
    }))
}

/// `POST /challenge/{id}/verify`: spend a challenge with the code that came with it.
pub fn verify_challenge(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    challenge_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let records = Records::new(graph, CHALLENGE);
    let now = text(payload, "now");
    let held = records.list(scope, 100)?;
    let Some(challenge) = held
        .iter()
        .find(|one| text(one, "challenge_id") == challenge_id)
        .cloned()
    else {
        // another person's challenge is not there to find
        return Err(StoreError::NotFound);
    };
    let expires = optional(&challenge, "expires_at").unwrap_or_default();
    if !expires.is_empty() && expires.as_str() < now.as_str() {
        records.delete(scope, field(&challenge, "id"))?;
        return Err(refused("challenge expired"));
    }
    if text(&challenge, "code") == text(payload, "code") {
        records.delete(scope, field(&challenge, "id"))?;
        return Ok(json!({"verified": true, "user_id": field(&challenge, "user_id")}));
    }
    // a wrong code does not spend the challenge, as shipped: the caller may try again
    Ok(json!({"verified": false}))
}

/// `GET /health`: whether this caller is enrolled, which is all the shipped health check added up.
pub fn enrolment_status(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
) -> Result<Value, StoreError> {
    Ok(json!({
        "status": "healthy",
        "service": "mfa-auth-service",
        "version": "2.0.0",
        "enrolled_users": if enrolment_of(graph, scope)?.is_some() { 1 } else { 0 },
    }))
}

/// `GET /backup-codes`: how many single-use codes remain on the caller's enrolment.
pub fn backup_remaining(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
) -> Result<Value, StoreError> {
    let remaining = enrolment_of(graph, scope)?
        .map(|held| {
            held.get("backup_codes")
                .and_then(Value::as_array)
                .map(|codes| codes.len())
                .unwrap_or(0)
        })
        .unwrap_or(0);
    Ok(json!({"remaining": remaining, "total": whole(&json!({"total": BACKUP_CODES}), "total")}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::MemoryGraph;

    fn mine() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    #[test]
    fn enrolling_gives_a_secret_a_qr_address_and_eight_codes() {
        let graph = MemoryGraph::new();
        let held = setup(
            &graph,
            &mine(),
            &json!({"user_id": "user-1", "now": "2026-01-01T00:00:00Z"}),
        )
        .expect("an enrolment");
        let secret = held["secret"].as_str().expect("a secret");
        // base32, so an authenticator app will read it
        assert!(secret
            .chars()
            .all(|letter| letter.is_ascii_uppercase() || ("234567").contains(letter)));
        assert_eq!(secret.len(), 32, "twenty bytes in base32");
        assert_eq!(held["backup_codes"].as_array().map(Vec::len), Some(8));
        assert_eq!(
            held["qr_uri"],
            json!(format!(
                "otpauth://totp/Vimbai:user-1?secret={secret}&issuer=Vimbai"
            ))
        );
        // enrolling again replaces rather than adds
        let again = setup(
            &graph,
            &mine(),
            &json!({"user_id": "user-1", "now": "2026-01-02T00:00:00Z"}),
        )
        .expect("an enrolment");
        assert_ne!(again["secret"], held["secret"]);
        assert_eq!(
            enrolment_status(&graph, &mine()).expect("a status")["enrolled_users"],
            json!(1)
        );
    }

    #[test]
    fn mfa_is_self_service_only() {
        let graph = MemoryGraph::new();
        let refused_setup = setup(&graph, &mine(), &json!({"user_id": "user-2"}));
        assert!(matches!(refused_setup, Err(StoreError::Forbidden(_))));
        setup(
            &graph,
            &mine(),
            &json!({"user_id": "user-1", "now": "2026-01-01T00:00:00Z"}),
        )
        .expect("an enrolment");
        let refused_verify = verify(
            &graph,
            &mine(),
            &json!({"user_id": "user-2", "code": "123456"}),
        );
        assert!(matches!(refused_verify, Err(StoreError::Forbidden(_))));
    }

    #[test]
    fn a_six_digit_code_is_accepted_and_anything_else_is_not() {
        let graph = MemoryGraph::new();
        // someone who never enrolled is told so rather than refused an unknown code
        let unenrolled = verify(&graph, &mine(), &json!({"code": "123456"})).expect("a check");
        assert_eq!(unenrolled["verified"], json!(false));
        assert_eq!(unenrolled["message"], json!("MFA not set up for this user"));

        setup(
            &graph,
            &mine(),
            &json!({"user_id": "user-1", "now": "2026-01-01T00:00:00Z"}),
        )
        .expect("an enrolment");
        // the shipped check took any six digits - pinned, not tightened, so a caller is not locked out
        let accepted_any = verify(
            &graph,
            &mine(),
            &json!({"code": "000000", "now": "2026-01-01T00:01:00Z"}),
        )
        .expect("a check");
        assert_eq!(accepted_any["verified"], json!(true));
        assert!(accepted_any["access_token"].as_str().is_some());
        let short = verify(&graph, &mine(), &json!({"code": "12345"})).expect("a check");
        assert_eq!(short["verified"], json!(false));
        assert_eq!(short["message"], json!("Invalid MFA code"));
        let letters = verify(&graph, &mine(), &json!({"code": "12a456"})).expect("a check");
        assert_eq!(letters["verified"], json!(false));
    }

    #[test]
    fn a_challenge_lasts_five_minutes_and_is_spent_when_it_verifies() {
        let graph = MemoryGraph::new();
        let raised = raise_challenge(
            &graph,
            &mine(),
            &json!({"user_id": "user-1", "now": "2026-01-01T00:00:00Z",
                    "now_plus_five": "2026-01-01T00:05:00Z"}),
        )
        .expect("a challenge");
        assert_eq!(raised["expires_in_minutes"], json!(5));
        assert_eq!(raised["method"], json!("totp"));
        let challenge_id = raised["challenge_id"].as_str().expect("an id").to_string();
        let code = text(
            &Records::new(&graph, CHALLENGE)
                .list(&mine(), 10)
                .expect("held")[0],
            "code",
        );
        assert_eq!(code.len(), 6);

        // a wrong code does not spend it
        let wrong = verify_challenge(
            &graph,
            &mine(),
            &challenge_id,
            &json!({"code": "000000", "now": "2026-01-01T00:01:00Z"}),
        )
        .expect("a check");
        assert_eq!(wrong["verified"], json!(false));
        let right = verify_challenge(
            &graph,
            &mine(),
            &challenge_id,
            &json!({"code": code, "now": "2026-01-01T00:02:00Z"}),
        )
        .expect("a check");
        assert_eq!(right["verified"], json!(true));
        assert_eq!(right["user_id"], json!("user-1"));
        // and once spent it is gone
        assert!(verify_challenge(
            &graph,
            &mine(),
            &challenge_id,
            &json!({"code": code, "now": "2026-01-01T00:03:00Z"})
        )
        .is_err());
    }

    #[test]
    fn an_expired_challenge_is_dropped_and_another_persons_is_missing() {
        let graph = MemoryGraph::new();
        let raised = raise_challenge(
            &graph,
            &mine(),
            &json!({"user_id": "user-1", "now": "2026-01-01T00:00:00Z",
                    "now_plus_five": "2026-01-01T00:05:00Z"}),
        )
        .expect("a challenge");
        let challenge_id = raised["challenge_id"].as_str().expect("an id").to_string();
        let theirs = BookScope::new("user-2", "book-2");
        assert!(
            verify_challenge(&graph, &theirs, &challenge_id, &json!({"code": "123456"})).is_err()
        );

        let code = text(
            &Records::new(&graph, CHALLENGE)
                .list(&mine(), 10)
                .expect("held")[0],
            "code",
        );
        let expired = verify_challenge(
            &graph,
            &mine(),
            &challenge_id,
            &json!({"code": code, "now": "2026-01-01T00:06:00Z"}),
        );
        assert!(expired.is_err());
        // it was dropped rather than left to be tried again
        assert!(Records::new(&graph, CHALLENGE)
            .list(&mine(), 10)
            .expect("held")
            .is_empty());
        // and a challenge cannot be raised for someone else
        assert!(raise_challenge(&graph, &mine(), &json!({"user_id": "user-2"})).is_err());
        // backup codes are counted, and none are used up by merely counting them
        setup(
            &graph,
            &mine(),
            &json!({"user_id": "user-1", "now": "2026-01-01T00:00:00Z"}),
        )
        .expect("an enrolment");
        let counted = backup_remaining(&graph, &mine()).expect("a count");
        assert_eq!(counted["remaining"], json!(8));
        assert_eq!(counted["total"], json!(8));
    }
}
