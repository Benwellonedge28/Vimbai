//! Parity cases captured from enterprise-sso-service before it was retired.

use serde_json::json;

use crate::clock::{parse_iso, to_iso};
use crate::crypto::{base64url_decode, base64url_encode, sha256_hex};
use crate::provider::Provider;
use crate::store::{SsoAuthRequest, SsoStore};
use crate::SsoError;

// 2026-11-03T08:18:26Z, the instant the reference capture ran.
const NOW: i64 = 1_793_693_906_000_000;

fn jwt(payload: &serde_json::Value) -> String {
    let header = base64url_encode(br#"{"alg":"none"}"#);
    let body = base64url_encode(payload.to_string().as_bytes());
    format!("{header}.{body}.sig")
}

fn oidc_request(token: String, provider: Provider) -> SsoAuthRequest {
    SsoAuthRequest {
        organization_id: "ORG1".to_string(),
        idp_token: token,
        provider,
    }
}

fn store_with(tag: &str) -> SsoStore {
    let mut store = SsoStore::new();
    store.issue("u-1", "ORG1", Provider::Oidc, NOW, tag);
    store
}

#[test]
fn sha256_matches_the_known_vectors() {
    assert_eq!(
        sha256_hex("abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(
        sha256_hex(""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    // the reference token was 64 hex characters, as this is
    assert_eq!(sha256_hex("u-1ORG11793693906.0nonce").len(), 64);
}

#[test]
fn base64url_round_trips_without_padding() {
    let payload = br#"{"sub":"u-1","exp":1793703600}"#;
    let encoded = base64url_encode(payload);
    assert!(!encoded.contains('='));
    assert_eq!(base64url_decode(&encoded).unwrap(), payload);
    // the service padded with 4 - len % 4; padding decodes the same here
    assert_eq!(
        base64url_decode(&format!("{encoded}====")).unwrap(),
        payload
    );
    assert!(base64url_decode("!!!!").is_none());
}

#[test]
fn iso_stamps_render_like_isoformat() {
    assert_eq!(to_iso(NOW), "2026-11-03T08:18:26+00:00");
    // microsecond precision appears only when it is non-zero
    assert_eq!(to_iso(NOW + 360_941), "2026-11-03T08:18:26.360941+00:00");
    assert_eq!(parse_iso("2026-11-03T08:18:26+00:00"), Some(NOW));
    assert_eq!(
        parse_iso("2026-11-03T08:18:26.360941Z"),
        Some(NOW + 360_941)
    );
    assert_eq!(
        to_iso(NOW + 8 * 3_600 * 1_000_000),
        "2026-11-03T16:18:26+00:00"
    );
}

#[test]
fn health_reports_the_service_identity() {
    let mut store = SsoStore::new();
    assert_eq!(
        store.health(),
        json!({
            "status": "healthy",
            "service": "enterprise-sso-service",
            "version": "2.0.0",
            "active_sessions": 0,
        })
    );
    store.issue("u-1", "ORG1", Provider::Oidc, NOW, "n");
    assert_eq!(store.health()["active_sessions"], 1);
}

#[test]
fn a_valid_oidc_token_issues_an_eight_hour_session() {
    let mut store = SsoStore::new();
    let token = jwt(&json!({"sub": "u-1", "exp": 1_793_694_506}));
    let outcome = store
        .authenticate(
            &oidc_request(token, Provider::Oidc),
            NOW,
            "nonce",
            "sso_fallback",
        )
        .unwrap();
    assert_eq!(outcome.user_id, "u-1");
    assert_eq!(outcome.organization_id, "ORG1");
    assert_eq!(outcome.provider, Provider::Oidc);
    assert_eq!(outcome.message, "SSO authentication successful");
    assert_eq!(outcome.expires_at, "2026-11-03T16:18:26+00:00");
    assert_eq!(outcome.vimbai_access_token.len(), 64);
    assert_eq!(
        outcome.to_json()["provider"],
        json!("oidc"),
        "the wire value, not the enum variant"
    );

    let session = store
        .validate_session(&outcome.vimbai_access_token, NOW)
        .unwrap();
    assert_eq!(session.user_id, "u-1");
    assert_eq!(session.org_id, "ORG1");
    assert_eq!(
        session.to_json(),
        json!({"valid": true, "user_id": "u-1", "org_id": "ORG1"})
    );
}

#[test]
fn azure_ad_and_okta_take_the_jwt_path() {
    for provider in [Provider::AzureAd, Provider::Okta] {
        let mut store = SsoStore::new();
        let token = jwt(&json!({"sub": "u-9"}));
        let outcome = store
            .authenticate(&oidc_request(token, provider), NOW, "nonce", "fallback")
            .unwrap();
        assert_eq!(outcome.user_id, "u-9");
        assert_eq!(outcome.to_json()["provider"], json!(provider.value()));
    }
}

#[test]
fn short_idp_tokens_are_refused_before_any_parsing() {
    let mut store = SsoStore::new();
    let err = store
        .authenticate(
            &oidc_request("short".to_string(), Provider::Oidc),
            NOW,
            "n",
            "f",
        )
        .unwrap_err();
    assert_eq!(err, SsoError::InvalidIdpToken);
    assert_eq!(err.status_code(), 401);
    assert_eq!(err.message(), "Invalid IdP token");
    assert_eq!(store.active_sessions(), 0);
}

#[test]
fn a_twelve_character_non_jwt_is_malformed() {
    let mut store = SsoStore::new();
    let err = store
        .authenticate(&oidc_request("a".repeat(12), Provider::Oidc), NOW, "n", "f")
        .unwrap_err();
    assert_eq!(err, SsoError::MalformedToken);
    assert_eq!(err.status_code(), 401);
}

#[test]
fn an_undecodable_payload_is_rejected() {
    let mut store = SsoStore::new();
    let token = format!("head.{}.sig", "!!!!");
    let err = store
        .authenticate(&oidc_request(token, Provider::Oidc), NOW, "n", "f")
        .unwrap_err();
    assert_eq!(err, SsoError::InvalidTokenPayload);
    assert_eq!(err.message(), "Invalid token payload");
}

#[test]
fn an_expired_claim_is_refused_with_its_own_message() {
    let mut store = SsoStore::new();
    let token = jwt(&json!({"sub": "x", "exp": 1000}));
    let err = store
        .authenticate(&oidc_request(token, Provider::Okta), NOW, "n", "f")
        .unwrap_err();
    assert_eq!(err, SsoError::TokenExpired);
    assert_eq!(err.message(), "Token expired");
}

#[test]
fn a_token_without_sub_gets_the_callers_fallback_id() {
    let mut store = SsoStore::new();
    let token = jwt(&json!({"name": "no subject"}));
    let outcome = store
        .authenticate(
            &oidc_request(token, Provider::Oidc),
            NOW,
            "n",
            "sso_ab12cd34",
        )
        .unwrap();
    assert_eq!(outcome.user_id, "sso_ab12cd34");
}

#[test]
fn saml_needs_twenty_characters_and_returns_fixed_claims() {
    let mut store = SsoStore::new();
    let short = store
        .authenticate(&oidc_request("x".repeat(15), Provider::Saml), NOW, "n", "f")
        .unwrap_err();
    assert_eq!(short, SsoError::InvalidSamlAssertion);
    assert_eq!(short.message(), "Invalid SAML assertion");

    let outcome = store
        .authenticate(&oidc_request("x".repeat(25), Provider::Saml), NOW, "n", "f")
        .unwrap();
    assert_eq!(outcome.user_id, "saml_user");
    assert_eq!(outcome.provider, Provider::Saml);
}

#[test]
fn an_unknown_provider_is_a_bad_request() {
    let err = SsoAuthRequest::from_value(&json!({
        "organization_id": "O",
        "idp_token": "x".repeat(20),
        "provider": "ldap",
    }))
    .unwrap_err();
    assert_eq!(err, SsoError::UnsupportedProvider("ldap".to_string()));
    assert_eq!(err.status_code(), 400);
    assert_eq!(err.message(), "Unsupported provider: ldap");
    // provider defaulted to oidc when the body omitted it
    let defaulted = SsoAuthRequest::from_value(&json!({"organization_id": "O"})).unwrap();
    assert_eq!(defaulted.provider, Provider::Oidc);
}

#[test]
fn sessions_expire_after_the_eight_hour_ttl() {
    let mut store = store_with("n");
    let issued = store.sessions_tokens();
    let session_token = issued[0].clone();

    // one microsecond before the expiry it is still good
    let expiry = NOW + crate::store::SESSION_TTL_MICROSECONDS;
    assert!(store.validate_session(&session_token, expiry).is_ok());
    let err = store
        .validate_session(&session_token, expiry + 1)
        .unwrap_err();
    assert_eq!(err, SsoError::SessionExpired);
    assert_eq!(err.message(), "Session expired");
    assert_eq!(
        store.active_sessions(),
        0,
        "the expired session was dropped"
    );
}

#[test]
fn unknown_and_revoked_tokens_share_one_message() {
    let mut store = store_with("n");
    let err = store.validate_session("nope", NOW).unwrap_err();
    assert_eq!(err, SsoError::InvalidSession);
    assert_eq!(err.message(), "Invalid or expired session");

    let token = store.sessions_tokens()[0].clone();
    assert!(store.revoke_session(&token));
    assert!(!store.revoke_session(&token), "second revoke is false");
    assert_eq!(
        store.validate_session(&token, NOW).unwrap_err(),
        SsoError::InvalidSession
    );
}

#[test]
fn configuration_is_echoed_and_not_stored() {
    let body = json!({"organization_id": "ORG1", "provider": "okta", "client_id": "cid"});
    assert_eq!(
        SsoStore::configure(&body),
        json!({"status": "configured", "organization_id": "ORG1", "provider": "okta"})
    );
}

#[test]
fn expired_sessions_can_be_pruned_in_one_pass() {
    let mut store = SsoStore::new();
    store.issue("a", "ORG1", Provider::Oidc, NOW, "one");
    store.issue(
        "b",
        "ORG1",
        Provider::Oidc,
        NOW + 9 * 3_600 * 1_000_000,
        "two",
    );
    let pruned = store.prune_expired(NOW + 9 * 3_600 * 1_000_000);
    assert_eq!(pruned, 1);
    assert_eq!(store.active_sessions(), 1);
}
