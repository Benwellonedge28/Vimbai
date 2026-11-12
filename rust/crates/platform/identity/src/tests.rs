//! The behaviour that matters, pinned against fixtures signed outside this crate
//! (OpenSSL / Python `cryptography`), so verification is proven against real
//! providers rather than against ourselves.

use serde_json::json;

use crate::claims::ClaimsPolicy;
use crate::discovery::DiscoveryDocument;
use crate::fixtures;
use crate::i18n::{accept_language, negotiate, Locale, MessageCatalogue, MessageKey};
use crate::idp::{IdpConfig, IdpRegistry, ProviderKind};
use crate::jose::{Alg, AlgPolicy, Jwk, Jwks};
use crate::saml::{parse_assertion, verify_signature, AssertionPolicy};
use crate::session::SessionPolicy;
use crate::store::{AuthRequest, Credential, SsoStore};

const NOW: i64 = fixtures::NOW_MICROS;

fn rsa_jwks() -> Jwks {
    Jwks::from_json(&format!("{{\"keys\":[{}]}}", fixtures::RSA_JWK_JSON)).unwrap()
}

fn ec_jwks() -> Jwks {
    Jwks::from_json(&format!("{{\"keys\":[{}]}}", fixtures::EC_JWK_JSON)).unwrap()
}

fn hmac_jwk() -> Jwk {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;
    let encoded = URL_SAFE_NO_PAD.encode(fixtures::HS256_TEST_SECRET.as_bytes());
    serde_json::from_value(json!({
        "kty": "oct",
        "kid": "hs-2026",
        "alg": "HS256",
        "k": encoded,
    }))
    .unwrap()
}

fn policy() -> ClaimsPolicy {
    ClaimsPolicy {
        issuer: "https://idp.example.com".to_string(),
        audiences: vec!["vimbai-api".to_string()],
        leeway_seconds: 30,
        expected_nonce: Some("n-abc123".to_string()),
        allowed_email_domains: vec!["example.com".to_string()],
        require_verified_email: true,
        group_roles: vec![
            ("finance-admin".to_string(), "finance_admin".to_string()),
            ("auditors".to_string(), "auditor".to_string()),
        ],
        algorithms: AlgPolicy::default(),
    }
}

fn rsa_provider() -> IdpConfig {
    IdpConfig::new(
        "org-1",
        "staff",
        ProviderKind::Okta,
        "https://idp.example.com",
        "vimbai-api",
    )
    .with_jwks(rsa_jwks())
    .with_region("zw")
    .with_locale(Locale::new("sn", Some("ZW")))
    .with_email_domains(&["example.com"])
    .with_group_roles(&[("finance-admin", "finance_admin"), ("auditors", "auditor")])
}

fn saml_provider() -> IdpConfig {
    let key: Jwk = serde_json::from_str(fixtures::RSA_JWK_JSON).unwrap();
    IdpConfig::new(
        "org-1",
        "partners",
        ProviderKind::Saml,
        "https://idp.example.com/saml",
        "vimbai-app",
    )
    .with_saml_key(key)
    .with_region("EU")
}

fn jwt_request(token: &str) -> AuthRequest {
    AuthRequest {
        organization_id: "org-1".to_string(),
        idp_alias: "staff".to_string(),
        credential: Credential::Jwt {
            token: token.to_string(),
            nonce: Some("n-abc123".to_string()),
        },
        now_micros: NOW,
        entropy: "test-entropy-0123456789abcdef".to_string(),
        address: None,
        accept_language: None,
    }
}

#[test]
fn an_rs256_token_from_an_outside_signer_verifies() {
    let identity =
        crate::claims::verify_and_map(fixtures::RS256_JWT, &rsa_jwks(), &policy(), NOW).unwrap();
    assert_eq!(identity.user_id, "user-4711");
    assert_eq!(identity.email.as_deref(), Some("Tariro.Moyo@Example.COM"));
    assert_eq!(identity.roles, vec!["auditor", "finance_admin"]);
    assert_eq!(identity.groups, vec!["finance-admin", "auditors"]);
    assert_eq!(identity.locale, Some(Locale::new("sn", Some("ZW"))));
    assert_eq!(identity.timezone.as_deref(), Some("Africa/Harare"));
}

#[test]
fn an_es256_token_from_an_outside_signer_verifies() {
    let identity =
        crate::claims::verify_and_map(fixtures::ES256_JWT, &ec_jwks(), &policy(), NOW).unwrap();
    assert_eq!(identity.user_id, "user-4711");
}

#[test]
fn an_hs256_token_verifies_only_when_the_secret_is_registered() {
    let jwks = Jwks {
        keys: vec![hmac_jwk()],
    };
    let mut symmetric = policy();
    symmetric.algorithms = AlgPolicy::permitting(vec![Alg::Hs256]);
    let identity =
        crate::claims::verify_and_map(fixtures::HS256_JWT, &jwks, &symmetric, NOW).unwrap();
    assert_eq!(identity.user_id, "user-4711");

    // the default policy refuses symmetric algorithms outright
    let refused =
        crate::claims::verify_and_map(fixtures::HS256_JWT, &jwks, &policy(), NOW).unwrap_err();
    assert_eq!(refused.code(), "unsupported_algorithm");
}

#[test]
fn a_tampered_payload_fails_the_signature() {
    let mut segments: Vec<&str> = fixtures::RS256_JWT.split('.').collect();
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;
    let forged = URL_SAFE_NO_PAD.encode(
        json!({
            "iss": "https://idp.example.com",
            "aud": ["vimbai-api"],
            "sub": "attacker",
            "exp": (NOW / 1_000_000) + 600,
            "email": "attacker@example.com",
            "email_verified": true,
            "nonce": "n-abc123"
        })
        .to_string(),
    );
    segments[1] = &forged;
    let tampered = segments.join(".");
    let error = crate::claims::verify_and_map(&tampered, &rsa_jwks(), &policy(), NOW).unwrap_err();
    assert_eq!(error.code(), "signature_invalid");
    assert_eq!(error.status_code(), 401);
}

#[test]
fn an_unsigned_token_is_refused() {
    let header = {
        use base64::engine::general_purpose::URL_SAFE_NO_PAD;
        use base64::Engine;
        URL_SAFE_NO_PAD.encode(json!({"alg": "none", "kid": "rsa-2026"}).to_string())
    };
    let token = format!(
        "{header}.{}.",
        fixtures::RS256_JWT.split('.').nth(1).unwrap_or("")
    );
    let error = crate::claims::verify_and_map(&token, &rsa_jwks(), &policy(), NOW).unwrap_err();
    assert!(
        [
            "unsupported_algorithm",
            "signature_invalid",
            "malformed_token"
        ]
        .contains(&error.code()),
        "got {}",
        error.code()
    );
}

#[test]
fn an_unknown_key_id_is_reported_as_such() {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;
    let header = URL_SAFE_NO_PAD.encode(json!({"alg": "RS256", "kid": "rotated-away"}).to_string());
    let payload = fixtures::RS256_JWT.split('.').nth(1).unwrap();
    let token = format!(
        "{header}.{payload}.{}",
        fixtures::RS256_JWT.split('.').nth(2).unwrap_or("")
    );
    let error = crate::claims::verify_and_map(&token, &rsa_jwks(), &policy(), NOW).unwrap_err();
    assert_eq!(error.code(), "unknown_signing_key");
}

#[test]
fn claim_rules_are_enforced() {
    let expired = crate::claims::verify_and_map(fixtures::EXPIRED_JWT, &rsa_jwks(), &policy(), NOW)
        .unwrap_err();
    assert_eq!(expired.code(), "token_expired");

    let wrong_audience =
        crate::claims::verify_and_map(fixtures::WRONG_AUDIENCE_JWT, &rsa_jwks(), &policy(), NOW)
            .unwrap_err();
    assert_eq!(wrong_audience.code(), "audience_mismatch");

    let wrong_issuer =
        crate::claims::verify_and_map(fixtures::WRONG_ISSUER_JWT, &rsa_jwks(), &policy(), NOW)
            .unwrap_err();
    assert_eq!(wrong_issuer.code(), "issuer_mismatch");
}

#[test]
fn a_missing_or_wrong_nonce_is_refused() {
    let mut expecting_other = policy();
    expecting_other.expected_nonce = Some("n-something-else".to_string());
    let error =
        crate::claims::verify_and_map(fixtures::RS256_JWT, &rsa_jwks(), &expecting_other, NOW)
            .unwrap_err();
    assert_eq!(error.code(), "nonce_mismatch");

    let mut requiring = policy();
    requiring.expected_nonce = Some("n-abc123".to_string());
    let ok = crate::claims::verify_and_map(fixtures::RS256_JWT, &rsa_jwks(), &requiring, NOW);
    assert!(ok.is_ok(), "the fixture carries the matching nonce");
}

#[test]
fn a_domain_outside_the_organisation_is_refused() {
    let mut strict = policy();
    strict.allowed_email_domains = vec!["vimbai.co.zw".to_string()];
    let error =
        crate::claims::verify_and_map(fixtures::RS256_JWT, &rsa_jwks(), &strict, NOW).unwrap_err();
    assert_eq!(error.code(), "email_domain_not_allowed");
    assert_eq!(error.status_code(), 403);
}

#[test]
fn leeway_covers_a_small_clock_difference_and_nothing_more() {
    let one_second_late = NOW + 1_000_000;
    assert!(crate::claims::verify_and_map(
        fixtures::RS256_JWT,
        &rsa_jwks(),
        &policy(),
        one_second_late
    )
    .is_ok());
    let well_late = NOW + 3_600 * 1_000_000;
    let error =
        crate::claims::verify_and_map(fixtures::RS256_JWT, &rsa_jwks(), &policy(), well_late)
            .unwrap_err();
    assert_eq!(error.code(), "token_expired");
}

#[test]
fn discovery_must_match_the_registration_and_use_tls() {
    let document = DiscoveryDocument::from_json(
        &json!({
            "issuer": "https://idp.example.com",
            "jwks_uri": "https://idp.example.com/keys",
            "authorization_endpoint": "https://idp.example.com/authorize",
            "id_token_signing_alg_values_supported": ["RS256", "ES256"],
            "scopes_supported": ["openid", "profile", "email"],
            "response_types_supported": ["code"],
        })
        .to_string(),
    )
    .unwrap();
    assert!(document
        .validate("https://idp.example.com", &AlgPolicy::default())
        .is_ok());

    let plaintext = DiscoveryDocument::from_json(
        &json!({"issuer": "https://idp.example.com", "jwks_uri": "http://idp.example.com/keys"})
            .to_string(),
    )
    .unwrap();
    assert_eq!(
        plaintext
            .validate("https://idp.example.com", &AlgPolicy::default())
            .unwrap_err()
            .code(),
        "discovery_invalid"
    );

    let wrong_issuer = DiscoveryDocument::from_json(
        &json!({"issuer": "https://other.example", "jwks_uri": "https://other.example/keys"})
            .to_string(),
    )
    .unwrap();
    assert_eq!(
        wrong_issuer
            .validate("https://idp.example.com", &AlgPolicy::default())
            .unwrap_err()
            .code(),
        "issuer_mismatch"
    );
}

#[test]
fn the_saml_assertion_is_parsed_and_its_signature_checked() {
    let assertion = parse_assertion(fixtures::SAML_ASSERTION_XML).unwrap();
    assert_eq!(assertion.id.as_deref(), Some("_a1b2c3"));
    assert_eq!(
        assertion.issuer.as_deref(),
        Some("https://idp.example.com/saml")
    );
    assert_eq!(assertion.name_id.as_deref(), Some("tariro@example.com"));
    assert_eq!(assertion.audiences, vec!["vimbai-app"]);
    assert_eq!(assertion.attribute("groups").len(), 2);
    assert_eq!(assertion.email().as_deref(), Some("tariro@example.com"));

    let key: Jwk = serde_json::from_str(fixtures::RSA_JWK_JSON).unwrap();
    verify_signature(
        fixtures::SAML_ASSERTION_XML.as_bytes(),
        fixtures::SAML_SIGNATURE_B64,
        std::slice::from_ref(&key),
    )
    .unwrap();

    // one byte changed and the signature no longer holds
    let tampered =
        fixtures::SAML_ASSERTION_XML.replace("tariro@example.com", "attacker@example.com");
    let error = verify_signature(
        tampered.as_bytes(),
        fixtures::SAML_SIGNATURE_B64,
        std::slice::from_ref(&key),
    )
    .unwrap_err();
    assert_eq!(error.code(), "signature_invalid");
}

#[test]
fn the_assertion_window_and_audience_are_enforced() {
    let assertion = parse_assertion(fixtures::SAML_ASSERTION_XML).unwrap();
    let domains: Vec<String> = vec!["example.com".to_string()];
    let good = AssertionPolicy {
        issuer: "https://idp.example.com/saml".to_string(),
        audiences: vec!["vimbai-app".to_string()],
        leeway_seconds: 30,
        allowed_email_domains: &domains,
        max_age_seconds: Some(300),
    };
    assert!(good.check(&assertion, NOW).is_ok());

    let wrong_audience = AssertionPolicy {
        issuer: good.issuer.clone(),
        audiences: vec!["another-app".to_string()],
        leeway_seconds: good.leeway_seconds,
        allowed_email_domains: good.allowed_email_domains,
        max_age_seconds: good.max_age_seconds,
    };
    assert_eq!(
        wrong_audience.check(&assertion, NOW).unwrap_err().code(),
        "audience_restriction_failed"
    );

    // 09:18:26Z + 30s leeway has passed an hour later
    let late = NOW + 3_700 * 1_000_000;
    assert_eq!(
        good.check(&assertion, late).unwrap_err().code(),
        "assertion_conditions_failed"
    );
}

#[test]
fn locales_are_negotiated_from_accept_language() {
    let ranked = accept_language("sn-ZW;q=0.9, fr;q=0.8, en;q=0.7");
    assert_eq!(ranked[0], Locale::new("sn", Some("ZW")));
    assert_eq!(ranked[1], Locale::new("fr", None));

    assert_eq!(negotiate(Some("sn-ZW,en;q=0.5"), None).language, "sn");
    assert_eq!(negotiate(Some("pt-BR"), None).language, "pt");
    assert_eq!(negotiate(Some("ja-JP"), None).language, "en", "falls back");
    assert_eq!(
        negotiate(None, Some(&Locale::new("fr", Some("FR")))).language,
        "fr"
    );
    // a zero-quality language is not chosen
    assert_eq!(negotiate(Some("fr;q=0"), None).language, "en");
}

#[test]
fn messages_are_localised_and_codes_never_are() {
    let error = crate::error::SsoError::with_detail(MessageKey::UnsupportedProvider, "ldap");
    assert_eq!(error.code(), "unsupported_provider");
    assert_eq!(error.message(), "Unsupported provider: ldap");
    let french = error.localized(&Locale::new("fr", None));
    assert!(french.starts_with("Fournisseur"), "{french}");
    let shona =
        crate::error::SsoError::new(MessageKey::TokenExpired).localized(&Locale::new("sn", None));
    assert_eq!(shona, "Chitupa chapera nguva");
    let body = error.to_json(&Locale::new("en", None));
    assert_eq!(body["code"], "unsupported_provider");
    assert!(MessageCatalogue::supported().contains(&"zh".to_string()));
}

#[test]
fn a_full_login_issues_a_session_carrying_region_and_language() {
    let mut store = SsoStore::new();
    store.register(rsa_provider());
    let outcome = store
        .authenticate(jwt_request(fixtures::RS256_JWT))
        .unwrap();

    assert_eq!(outcome.identity.user_id, "user-4711");
    assert_eq!(outcome.session.roles, vec!["auditor", "finance_admin"]);
    assert_eq!(outcome.session.region, "ZW");
    // the person's own locale from the provider wins over the org default
    assert_eq!(outcome.locale, Locale::new("sn", Some("ZW")));
    assert_eq!(outcome.session.token.len(), 64);

    let body = outcome.to_json();
    assert_eq!(body["token_type"], "Bearer");
    assert_eq!(body["message"], "SSO authentication successful");
    assert_eq!(body["provider"], "staff");
    assert_eq!(body["region"], "ZW");
    assert_eq!(body["locale"], "sn-ZW");
    assert!(body["expires_at"].as_str().unwrap().ends_with("+00:00"));
}

#[test]
fn an_unknown_or_disabled_provider_is_rejected_before_verification() {
    let mut store = SsoStore::new();
    store.register(rsa_provider());

    let mut request = jwt_request(fixtures::RS256_JWT);
    request.idp_alias = "ghost".to_string();
    assert_eq!(
        store.authenticate(request).unwrap_err().code(),
        "idp_not_configured"
    );

    store.registry.set_enabled("org-1", "staff", false);
    let error = store
        .authenticate(jwt_request(fixtures::RS256_JWT))
        .unwrap_err();
    assert_eq!(error.code(), "idp_disabled");
    assert_eq!(error.status_code(), 400);
}

#[test]
fn sessions_expire_idle_out_and_rotate() {
    let mut store = SsoStore::new();
    store.register(rsa_provider());
    let outcome = store
        .authenticate(jwt_request(fixtures::RS256_JWT))
        .unwrap();
    let token = outcome.session.token.clone();

    // inside the idle window the token still works, and rotation retires it
    let (session, retired) = store
        .sessions
        .validate(
            &token,
            NOW + 60 * 1_000_000,
            &SessionPolicy::default(),
            None,
            "second",
        )
        .unwrap();
    assert_eq!(retired.as_deref(), Some(token.as_str()));
    assert_ne!(session.token, token);
    assert_eq!(session.rotated_from.as_deref(), Some(token.as_str()));
    assert_eq!(
        store
            .sessions
            .validate(
                &token,
                NOW + 61 * 1_000_000,
                &SessionPolicy::default(),
                None,
                "third"
            )
            .unwrap_err()
            .code(),
        "invalid_session",
        "the retired token is dead"
    );

    // thirty minutes of silence ends it
    let rotated = session.token.clone();
    let idle = store
        .sessions
        .validate(
            &rotated,
            NOW + 61 * 1_000_000 + 31 * 60 * 1_000_000,
            &SessionPolicy::default(),
            None,
            "fourth",
        )
        .unwrap_err();
    assert_eq!(idle.code(), "session_idle");
}

#[test]
fn the_absolute_lifetime_ends_a_busy_session() {
    let mut store = SsoStore::new();
    store.register(rsa_provider());
    let outcome = store
        .authenticate(jwt_request(fixtures::RS256_JWT))
        .unwrap();
    let mut token = outcome.session.token.clone();
    let mut now = NOW;

    // a busy client, touching the session every five minutes for nine hours
    for step in 0..108 {
        now += 5 * 60 * 1_000_000;
        let result = store.sessions.validate(
            &token,
            now,
            &SessionPolicy::default(),
            None,
            &format!("entropy-{step}"),
        );
        match result {
            Ok((session, _)) => token = session.token,
            Err(error) => {
                assert!(step >= 95, "ended early at step {step}");
                assert_eq!(error.code(), "session_expired");
                return;
            }
        }
    }
    panic!("a session outlived its eight-hour limit");
}

#[test]
fn a_strict_policy_binds_the_address_and_caps_sessions() {
    let mut store = SsoStore::new();
    let mut config = rsa_provider();
    config.session = SessionPolicy::strict();
    store.register(config);

    let mut request = jwt_request(fixtures::RS256_JWT);
    request.address = Some("196.43.10.1".to_string());
    let first = store.authenticate(request.clone()).unwrap();

    let error = store
        .sessions
        .validate(
            &first.session.token,
            NOW + 1_000_000,
            &SessionPolicy::strict(),
            Some("10.0.0.9"),
            "entropy-x",
        )
        .unwrap_err();
    assert_eq!(error.code(), "session_ip_mismatch");

    // a second login from the same address is fine; the cap is three
    let _second = store.authenticate(request.clone()).unwrap();
    let _third = store.authenticate(request.clone()).unwrap();
    assert_eq!(store.sessions.active(), 3);
    let _fourth = store.authenticate(request).unwrap();
    assert_eq!(store.sessions.active(), 3, "the oldest was evicted");
    assert!(store
        .sessions
        .audit()
        .iter()
        .any(|event| event.code.as_deref() == Some("concurrent_session_evicted")));
}

#[test]
fn revocation_is_recorded_and_can_be_applied_to_a_whole_user() {
    let mut store = SsoStore::new();
    store.register(rsa_provider());
    let first = store
        .authenticate(jwt_request(fixtures::RS256_JWT))
        .unwrap();
    let _second = store
        .authenticate(jwt_request(fixtures::RS256_JWT))
        .unwrap();

    assert!(store.revoke_session(&first.session.token, NOW));
    assert!(
        !store.revoke_session(&first.session.token, NOW),
        "already gone"
    );
    assert_eq!(store.revoke_all_for_user("user-4711", NOW), 1);
    assert_eq!(store.sessions.active(), 0);

    let codes: Vec<&str> = store
        .sessions
        .audit()
        .iter()
        .map(|event| event.kind.code())
        .collect();
    assert!(codes.contains(&"authenticated"));
    assert!(codes.contains(&"session_revoked"));
    assert!(codes.contains(&"sessions_revoked_all"));
}

#[test]
fn rejections_are_audited_with_their_code() {
    let mut store = SsoStore::new();
    store.register(rsa_provider());
    let _ = store
        .authenticate(jwt_request(fixtures::EXPIRED_JWT))
        .unwrap_err();
    let rejected = store
        .sessions
        .audit()
        .iter()
        .find(|event| event.kind.code() == "authentication_rejected")
        .expect("rejection recorded");
    assert_eq!(rejected.code.as_deref(), Some("token_expired"));
    assert!(rejected.to_json()["at"]
        .as_str()
        .unwrap()
        .ends_with("+00:00"));
}

#[test]
fn a_saml_login_works_end_to_end() {
    let mut store = SsoStore::new();
    store.register(saml_provider());
    let request = AuthRequest {
        organization_id: "org-1".to_string(),
        idp_alias: "partners".to_string(),
        credential: Credential::Saml {
            assertion_xml: fixtures::SAML_ASSERTION_XML.to_string(),
            signature_b64: fixtures::SAML_SIGNATURE_B64.to_string(),
            in_response_to: Some("idp-request-1234".to_string()),
        },
        now_micros: NOW,
        entropy: "test-entropy-0123456789abcdef".to_string(),
        address: None,
        accept_language: Some("fr-FR,en;q=0.5".to_string()),
    };
    let outcome = store.authenticate(request).unwrap();
    assert_eq!(outcome.identity.user_id, "tariro@example.com");
    assert_eq!(outcome.session.region, "EU");
    assert_eq!(
        outcome.locale.language, "fr",
        "request language, no claim locale"
    );
    assert_eq!(
        outcome.session.roles.len(),
        0,
        "no group mapping configured"
    );
}

#[test]
fn a_credential_of_the_wrong_family_is_refused() {
    let mut store = SsoStore::new();
    store.register(saml_provider());
    let mut request = jwt_request(fixtures::RS256_JWT);
    request.idp_alias = "partners".to_string();
    let error = store.authenticate(request).unwrap_err();
    assert_eq!(error.code(), "invalid_saml_assertion");

    let mut store = SsoStore::new();
    store.register(rsa_provider());
    let mut saml_request = jwt_request(fixtures::RS256_JWT);
    saml_request.credential = Credential::Saml {
        assertion_xml: fixtures::SAML_ASSERTION_XML.to_string(),
        signature_b64: fixtures::SAML_SIGNATURE_B64.to_string(),
        in_response_to: None,
    };
    assert_eq!(
        store.authenticate(saml_request).unwrap_err().code(),
        "malformed_token"
    );
}

#[test]
fn a_registry_keeps_organisations_and_aliases_apart() {
    let mut registry = IdpRegistry::new();
    registry.register(rsa_provider());
    let mut other = saml_provider();
    other.organization_id = "org-2".to_string();
    registry.register(other);

    assert_eq!(registry.len(), 2);
    assert!(registry.get("org-1", "staff").is_ok());
    assert!(
        registry.get("org-2", "staff").is_err(),
        "alias is per organisation"
    );
    assert_eq!(registry.for_organization("org-1").len(), 1);
    let advertised = registry.advertised("org-1");
    assert_eq!(advertised[0]["alias"], "staff");
    assert_eq!(advertised[0]["region"], "ZW");
    assert_eq!(advertised[0]["locale"], "sn-ZW");
}

#[test]
fn health_reports_providers_and_sessions() {
    let mut store = SsoStore::new();
    store.register(rsa_provider());
    store
        .authenticate(jwt_request(fixtures::RS256_JWT))
        .unwrap();
    let health = store.health();
    assert_eq!(health["status"], "healthy");
    assert_eq!(health["version"], "3.0.0");
    assert_eq!(health["active_sessions"], 1);
    assert_eq!(health["identity_providers"], 1);
}

#[test]
fn pruning_clears_expired_sessions() {
    let mut store = SsoStore::new();
    store.register(rsa_provider());
    store
        .authenticate(jwt_request(fixtures::RS256_JWT))
        .unwrap();
    assert_eq!(store.prune(NOW + 1_000_000), 0);
    assert_eq!(store.prune(NOW + 9 * 3_600 * 1_000_000), 1);
    assert_eq!(store.sessions.active(), 0);
}
