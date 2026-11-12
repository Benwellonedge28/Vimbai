//! What a caller actually uses: register providers, authenticate a credential,
//! keep a session, and get an audited answer in the user's language.

use serde_json::{json, Value};

use crate::claims::{ClaimsPolicy, Identity};
use crate::error::SsoError;
use crate::i18n::{negotiate, Locale, MessageKey};
use crate::idp::{IdpConfig, IdpRegistry, ProviderKind};
use crate::saml::{self, AssertionPolicy};
use crate::session::{AuditEvent, AuditKind, Session, SessionStore};

/// What the identity provider handed the client.
#[derive(Debug, Clone, PartialEq)]
pub enum Credential {
    /// An OIDC ID token or access token.
    Jwt {
        token: String,
        nonce: Option<String>,
    },
    /// A SAML assertion with its signature over the assertion bytes.
    Saml {
        assertion_xml: String,
        signature_b64: String,
        in_response_to: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct AuthRequest {
    pub organization_id: String,
    pub idp_alias: String,
    pub credential: Credential,
    pub now_micros: i64,
    /// At least 32 bytes of unpredictable data for the session token. The caller
    /// owns the randomness; tests pass a fixed string.
    pub entropy: String,
    pub address: Option<String>,
    /// The request's preferred language, already the raw `Accept-Language` value.
    pub accept_language: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AuthOutcome {
    pub session: Session,
    pub identity: Identity,
    pub locale: Locale,
}

impl AuthOutcome {
    /// The body the authentication endpoint returns.
    pub fn to_json(&self) -> Value {
        json!({
            "vimbai_access_token": self.session.token,
            "access_token": self.session.token,
            "token_type": "Bearer",
            "user_id": self.identity.user_id,
            "organization_id": self.session.organization_id,
            "provider": self.session.idp_alias,
            "roles": self.identity.roles,
            "locale": self.locale.to_string(),
            "timezone": self.identity.timezone,
            "region": self.session.region,
            "expires_at": self.session.expires_at(),
            "message": "SSO authentication successful",
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct SsoStore {
    pub registry: IdpRegistry,
    pub sessions: SessionStore,
}

impl SsoStore {
    pub fn new() -> Self {
        SsoStore::default()
    }

    pub fn register(&mut self, config: IdpConfig) {
        self.registry.register(config);
    }

    pub fn health(&self) -> Value {
        json!({
            "status": "healthy",
            "service": "enterprise-sso",
            "version": "3.0.0",
            "active_sessions": self.sessions.active(),
            "identity_providers": self.registry.len(),
        })
    }

    /// The providers a login page should offer.
    pub fn advertised_providers(&self, organization_id: &str) -> Value {
        json!({
            "organization_id": organization_id,
            "providers": self.registry.advertised(organization_id),
        })
    }

    /// Authenticate. Every rejection is recorded, so a run of failures is
    /// visible rather than silent.
    pub fn authenticate(&mut self, request: AuthRequest) -> Result<AuthOutcome, SsoError> {
        let organization_locale = self
            .registry
            .get(&request.organization_id, &request.idp_alias)
            .map(|config| config.default_locale.clone())
            .ok();
        let locale = negotiate(
            request.accept_language.as_deref(),
            organization_locale.as_ref(),
        );

        match self.authenticate_inner(&request, &locale) {
            Ok(outcome) => Ok(outcome),
            Err(error) => {
                self.sessions.record(AuditEvent {
                    at_micros: request.now_micros,
                    kind: AuditKind::AuthenticationRejected,
                    organization_id: request.organization_id.clone(),
                    idp_alias: request.idp_alias.clone(),
                    user_id: None,
                    code: Some(error.code().to_string()),
                });
                Err(error)
            }
        }
    }

    fn authenticate_inner(
        &mut self,
        request: &AuthRequest,
        locale: &Locale,
    ) -> Result<AuthOutcome, SsoError> {
        let config = self
            .registry
            .get(&request.organization_id, &request.idp_alias)?
            .clone();

        let (identity, claims_locale) = match (&config.kind, &request.credential) {
            (ProviderKind::Saml, Credential::Saml { .. }) => {
                let identity = self.authenticate_saml(&config, request)?;
                (identity, None)
            }
            (ProviderKind::Saml, Credential::Jwt { .. }) => {
                return Err(SsoError::with_detail(
                    MessageKey::InvalidSamlAssertion,
                    "provider expects a SAML assertion",
                ))
            }
            (_, Credential::Saml { .. }) => {
                return Err(SsoError::with_detail(
                    MessageKey::MalformedToken,
                    "provider expects an OIDC token",
                ))
            }
            (_, Credential::Jwt { token, nonce }) => {
                let policy = config.claims_policy_with_nonce(nonce.as_deref());
                let identity = crate::claims::verify_and_map(
                    token,
                    &config.jwks,
                    &policy,
                    request.now_micros,
                )?;
                let identity_locale = identity.locale.clone();
                (identity, identity_locale)
            }
        };

        let session_locale = claims_locale
            .or_else(|| Some(locale.clone()))
            .unwrap_or_else(|| config.default_locale.clone());

        let session = self.sessions.open(
            &config.session,
            &identity.user_id,
            &config.organization_id,
            &config.alias,
            identity.roles.clone(),
            session_locale.clone(),
            identity.timezone.clone(),
            &config.region,
            request.now_micros,
            &format!("{}{}", request.entropy, request.idp_alias),
            request.address.clone(),
        );
        self.sessions.record(AuditEvent {
            at_micros: request.now_micros,
            kind: AuditKind::Authenticated,
            organization_id: config.organization_id.clone(),
            idp_alias: config.alias.clone(),
            user_id: Some(identity.user_id.clone()),
            code: None,
        });

        Ok(AuthOutcome {
            session,
            identity,
            locale: session_locale,
        })
    }

    fn authenticate_saml(
        &self,
        config: &IdpConfig,
        request: &AuthRequest,
    ) -> Result<Identity, SsoError> {
        let Credential::Saml {
            assertion_xml,
            signature_b64,
            in_response_to,
        } = &request.credential
        else {
            unreachable!("caller matched the credential kind");
        };
        if let Some(expected) = in_response_to {
            if expected.len() < 8 {
                return Err(SsoError::with_detail(
                    MessageKey::InvalidSamlAssertion,
                    "InResponseTo too short",
                ));
            }
        }
        saml::verify_signature(assertion_xml.as_bytes(), signature_b64, &config.saml_keys)?;
        let assertion = saml::parse_assertion(assertion_xml)?;
        let policy = AssertionPolicy {
            issuer: config.issuer.clone(),
            audiences: config.audiences.clone(),
            leeway_seconds: config.claims.leeway_seconds,
            allowed_email_domains: &config.claims.allowed_email_domains,
            max_age_seconds: Some(300),
        };
        policy.check(&assertion, request.now_micros)?;

        let user_id = assertion
            .name_id
            .clone()
            .ok_or_else(|| SsoError::with_detail(MessageKey::MissingClaim, "NameID"))?;
        let groups = assertion.attribute("groups");
        let roles = map_roles(&groups, &config.claims);
        Ok(Identity {
            user_id,
            email: assertion.email(),
            roles,
            groups,
            locale: None,
            timezone: None,
            display_name: assertion.attribute("displayName").into_iter().next(),
        })
    }

    /// Validate a session, rotating the token when the policy asks. The caller
    /// must persist the new token: the old one is dead.
    pub fn validate_session(
        &mut self,
        token: &str,
        organization_id: &str,
        idp_alias: &str,
        now_micros: i64,
        address: Option<&str>,
        entropy: &str,
    ) -> Result<Session, SsoError> {
        let policy = self
            .registry
            .get(organization_id, idp_alias)
            .map(|config| config.session.clone())
            .unwrap_or_default();
        let (session, _retired) = self
            .sessions
            .validate(token, now_micros, &policy, address, entropy)?;
        Ok(session)
    }

    pub fn revoke_session(&mut self, token: &str, now_micros: i64) -> bool {
        self.sessions.revoke(token, now_micros)
    }

    pub fn revoke_all_for_user(&mut self, user_id: &str, now_micros: i64) -> usize {
        self.sessions.revoke_all_for_user(user_id, now_micros)
    }

    pub fn prune(&mut self, now_micros: i64) -> usize {
        self.sessions.prune(now_micros)
    }
}

fn map_roles(groups: &[String], policy: &ClaimsPolicy) -> Vec<String> {
    let mut roles: Vec<String> = policy
        .group_roles
        .iter()
        .filter(|(group, _)| groups.iter().any(|held| held == group))
        .map(|(_, role)| role.clone())
        .collect();
    roles.sort();
    roles.dedup();
    roles
}
