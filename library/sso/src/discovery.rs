//! OpenID Connect discovery: the document that tells us where a provider's keys
//! live and which algorithms it signs with.

use serde::Deserialize;
use serde_json::Value;

use crate::error::SsoError;
use crate::i18n::MessageKey;
use crate::jose::AlgPolicy;

#[derive(Debug, Clone, Deserialize)]
pub struct DiscoveryDocument {
    pub issuer: String,
    #[serde(default)]
    pub authorization_endpoint: Option<String>,
    #[serde(default)]
    pub token_endpoint: Option<String>,
    #[serde(default)]
    pub userinfo_endpoint: Option<String>,
    #[serde(default)]
    pub end_session_endpoint: Option<String>,
    #[serde(default)]
    pub jwks_uri: Option<String>,
    #[serde(default)]
    pub scopes_supported: Vec<String>,
    #[serde(default)]
    pub response_types_supported: Vec<String>,
    #[serde(default)]
    pub id_token_signing_alg_values_supported: Vec<String>,
    #[serde(default)]
    pub claims_supported: Vec<String>,
}

impl DiscoveryDocument {
    pub fn from_json(raw: &str) -> Result<DiscoveryDocument, SsoError> {
        let value: Value =
            serde_json::from_str(raw).map_err(|_| SsoError::new(MessageKey::DiscoveryInvalid))?;
        // A document that is not an object, or has no issuer, is unusable.
        if !value.is_object() {
            return Err(SsoError::new(MessageKey::DiscoveryInvalid));
        }
        serde_json::from_value(value).map_err(|_| SsoError::new(MessageKey::DiscoveryInvalid))
    }

    /// Check the document against the organisation's registration: the issuer
    /// must match exactly (that is the point of the field), the key endpoint
    /// must be TLS, and at least one algorithm we accept must be offered.
    pub fn validate(&self, expected_issuer: &str, policy: &AlgPolicy) -> Result<(), SsoError> {
        if self.issuer.trim_end_matches('/') != expected_issuer.trim_end_matches('/') {
            return Err(SsoError::with_detail(
                MessageKey::IssuerMismatch,
                self.issuer.clone(),
            ));
        }
        let jwks_uri = self.jwks_uri.as_deref().ok_or_else(|| {
            SsoError::with_detail(MessageKey::DiscoveryInvalid, "jwks_uri missing")
        })?;
        if !jwks_uri.starts_with("https://") {
            return Err(SsoError::with_detail(
                MessageKey::DiscoveryInvalid,
                "jwks_uri must be https",
            ));
        }
        if !self.id_token_signing_alg_values_supported.is_empty() {
            let offered = self
                .id_token_signing_alg_values_supported
                .iter()
                .filter_map(|name| crate::jose::Alg::parse(name))
                .any(|alg| policy.allows(alg));
            if !offered {
                return Err(SsoError::with_detail(
                    MessageKey::UnsupportedAlgorithm,
                    self.id_token_signing_alg_values_supported.join(","),
                ));
            }
        }
        Ok(())
    }
}
