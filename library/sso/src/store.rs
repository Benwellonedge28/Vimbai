//! Session lifecycle: authenticate against an IdP payload, keep the resulting
//! session, validate it, revoke it.
//!
//! The service kept `_session_store` as a module global with no tenant
//! dimension, so any caller who guessed a token saw the session. The store here
//! is owned by the caller, so each Book keeps its own sessions.

use indexmap::IndexMap;
use serde_json::{json, Map, Value};

use crate::clock::{to_iso, MICROSECONDS_PER_HOUR};
use crate::crypto::sha256_hex;
use crate::error::SsoError;
use crate::provider::Provider;

/// Eight hours, as the service issued.
pub const SESSION_TTL_MICROSECONDS: i64 = 8 * MICROSECONDS_PER_HOUR;

#[derive(Debug, Clone, PartialEq)]
pub struct SsoAuthRequest {
    pub organization_id: String,
    pub idp_token: String,
    pub provider: Provider,
}

impl SsoAuthRequest {
    /// Parse a wire body. An unknown provider is a 400, not a parse failure.
    pub fn from_value(body: &Value) -> Result<SsoAuthRequest, SsoError> {
        let provider_value = body
            .get("provider")
            .and_then(Value::as_str)
            .unwrap_or("oidc");
        let provider = Provider::parse(provider_value)
            .ok_or_else(|| SsoError::UnsupportedProvider(provider_value.to_string()))?;
        Ok(SsoAuthRequest {
            organization_id: body
                .get("organization_id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            idp_token: body
                .get("idp_token")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            provider,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AuthOutcome {
    pub vimbai_access_token: String,
    pub user_id: String,
    pub organization_id: String,
    pub expires_at: String,
    pub message: String,
    pub provider: Provider,
}

impl AuthOutcome {
    pub fn to_json(&self) -> Value {
        json!({
            "vimbai_access_token": self.vimbai_access_token,
            "user_id": self.user_id,
            "organization_id": self.organization_id,
            "expires_at": self.expires_at,
            "message": self.message,
            "provider": self.provider.value(),
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Session {
    pub token: String,
    pub user_id: String,
    pub org_id: String,
    pub provider: Provider,
    pub created_at_micros: i64,
    pub expires_at_micros: i64,
}

impl Session {
    pub fn expires_at(&self) -> String {
        to_iso(self.expires_at_micros)
    }

    /// The body `/session/{token}` returned for a live session.
    pub fn to_json(&self) -> Value {
        json!({"valid": true, "user_id": self.user_id, "org_id": self.org_id})
    }
}

#[derive(Debug, Clone, Default)]
pub struct SsoStore {
    sessions: IndexMap<String, Session>,
}

impl SsoStore {
    pub fn new() -> Self {
        SsoStore::default()
    }

    pub fn active_sessions(&self) -> usize {
        self.sessions.len()
    }

    pub fn session(&self, token: &str) -> Option<&Session> {
        self.sessions.get(token)
    }

    /// The session tokens currently held, in insertion order.
    pub fn sessions_tokens(&self) -> Vec<String> {
        self.sessions.keys().cloned().collect()
    }

    /// The `/health` body.
    pub fn health(&self) -> Value {
        json!({
            "status": "healthy",
            "service": "enterprise-sso-service",
            "version": "2.0.0",
            "active_sessions": self.active_sessions(),
        })
    }

    /// `/config` echoed the request and stored nothing, so no configuration
    /// survives here either; the caller owns provider settings.
    pub fn configure(body: &Value) -> Value {
        json!({
            "status": "configured",
            "organization_id": body.get("organization_id").cloned().unwrap_or(Value::Null),
            "provider": body.get("provider").cloned().unwrap_or(Value::Null),
        })
    }

    /// `/auth/sso`: validate the IdP material, then issue a Vimbai session.
    ///
    /// `now_micros` and `nonce` are supplied so the derived token and the expiry
    /// stamp are reproducible; `fallback_id_prefix` stands in for the random id
    /// the service minted when the token carried no `sub`.
    pub fn authenticate(
        &mut self,
        request: &SsoAuthRequest,
        now_micros: i64,
        nonce: &str,
        fallback_id_prefix: &str,
    ) -> Result<AuthOutcome, SsoError> {
        if request.idp_token.len() < 10 {
            return Err(SsoError::InvalidIdpToken);
        }

        let user_id = if request.provider.speaks_jwt() {
            let claims = validate_oidc_token(&request.idp_token, now_micros)?;
            claims
                .get("sub")
                .and_then(Value::as_str)
                .map(ToString::to_string)
                .unwrap_or_else(|| fallback_id_prefix.to_string())
        } else {
            validate_saml_assertion(&request.idp_token)?;
            "saml_user".to_string()
        };

        Ok(self.issue(
            &user_id,
            &request.organization_id,
            request.provider,
            now_micros,
            nonce,
        ))
    }

    /// Issue without validating anything, for callers that already checked the
    /// IdP material themselves.
    pub fn issue(
        &mut self,
        user_id: &str,
        organization_id: &str,
        provider: Provider,
        now_micros: i64,
        nonce: &str,
    ) -> AuthOutcome {
        let token = sha256_hex(&format!(
            "{user_id}{organization_id}{}{nonce}",
            now_micros as f64 / 1_000_000.0
        ));
        let expires_at_micros = now_micros + SESSION_TTL_MICROSECONDS;
        let session = Session {
            token: token.clone(),
            user_id: user_id.to_string(),
            org_id: organization_id.to_string(),
            provider,
            created_at_micros: now_micros,
            expires_at_micros,
        };
        let expires_at = session.expires_at();
        self.sessions.insert(token.clone(), session);
        AuthOutcome {
            vimbai_access_token: token,
            user_id: user_id.to_string(),
            organization_id: organization_id.to_string(),
            expires_at,
            message: "SSO authentication successful".to_string(),
            provider,
        }
    }

    /// `/session/{token}`: a live session, or the same 401s the service sent.
    /// An expired session is dropped on the way out.
    pub fn validate_session(&mut self, token: &str, now_micros: i64) -> Result<Session, SsoError> {
        let session = self
            .sessions
            .get(token)
            .cloned()
            .ok_or(SsoError::InvalidSession)?;
        if session.expires_at_micros < now_micros {
            self.sessions.shift_remove(token);
            return Err(SsoError::SessionExpired);
        }
        Ok(session)
    }

    /// `/session/{token}` on DELETE: `true` when a session was revoked.
    pub fn revoke_session(&mut self, token: &str) -> bool {
        self.sessions.shift_remove(token).is_some()
    }

    /// Drop every session whose expiry has passed.
    pub fn prune_expired(&mut self, now_micros: i64) -> usize {
        let expired: Vec<String> = self
            .sessions
            .iter()
            .filter(|(_, session)| session.expires_at_micros < now_micros)
            .map(|(token, _)| token.clone())
            .collect();
        for token in &expired {
            self.sessions.shift_remove(token);
        }
        expired.len()
    }
}

/// The OIDC path: a three-part JWT whose payload decodes and has not expired.
/// Signatures were never checked in the service, and the port does not pretend
/// otherwise: callers must verify them upstream.
pub fn validate_oidc_token(token: &str, now_micros: i64) -> Result<Map<String, Value>, SsoError> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return Err(SsoError::MalformedToken);
    }
    let payload_bytes =
        crate::crypto::base64url_decode(parts[1]).ok_or(SsoError::InvalidTokenPayload)?;
    let payload: Value =
        serde_json::from_slice(&payload_bytes).map_err(|_| SsoError::InvalidTokenPayload)?;
    let claims = payload.as_object().ok_or(SsoError::InvalidTokenPayload)?;
    if let Some(exp) = claims.get("exp") {
        let seconds = exp.as_f64().ok_or(SsoError::InvalidTokenPayload)?;
        if seconds < now_micros as f64 / 1_000_000.0 {
            return Err(SsoError::TokenExpired);
        }
    }
    Ok(claims.clone())
}

/// The SAML path: the service only checked length and returned fixed claims.
pub fn validate_saml_assertion(token: &str) -> Result<Value, SsoError> {
    if token.len() < 20 {
        return Err(SsoError::InvalidSamlAssertion);
    }
    Ok(json!({
        "sub": "saml_user",
        "name": "SAML User",
        "email": "user@saml.example",
    }))
}
