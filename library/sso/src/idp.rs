//! Registered identity providers, one registry per deployment, keyed by
//! organisation and alias so a group with a Zimbabwean and a European provider
//! keeps them apart.

use indexmap::IndexMap;

use crate::claims::ClaimsPolicy;
use crate::discovery::DiscoveryDocument;
use crate::error::SsoError;
use crate::i18n::{Locale, MessageKey};
use crate::jose::{AlgPolicy, Jwk, Jwks};
use crate::session::SessionPolicy;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProviderKind {
    Oidc,
    Saml,
    AzureAd,
    Okta,
    Google,
}

impl ProviderKind {
    pub fn value(self) -> &'static str {
        match self {
            ProviderKind::Oidc => "oidc",
            ProviderKind::Saml => "saml",
            ProviderKind::AzureAd => "azure_ad",
            ProviderKind::Okta => "okta",
            ProviderKind::Google => "google",
        }
    }

    pub fn parse(value: &str) -> Option<ProviderKind> {
        match value {
            "oidc" => Some(ProviderKind::Oidc),
            "saml" => Some(ProviderKind::Saml),
            "azure_ad" | "azuread" => Some(ProviderKind::AzureAd),
            "okta" => Some(ProviderKind::Okta),
            "google" => Some(ProviderKind::Google),
            _ => None,
        }
    }

    pub fn speaks_jwt(self) -> bool {
        !matches!(self, ProviderKind::Saml)
    }
}

/// Everything known about one provider for one organisation.
#[derive(Debug, Clone)]
pub struct IdpConfig {
    pub organization_id: String,
    /// The name clients use to pick this provider, e.g. `staff` or `partners`.
    pub alias: String,
    pub kind: ProviderKind,
    pub issuer: String,
    pub audiences: Vec<String>,
    pub client_id: Option<String>,
    /// The key set, as fetched from the provider's `jwks_uri`.
    pub jwks: Jwks,
    pub discovery: Option<DiscoveryDocument>,
    pub claims: ClaimsPolicy,
    /// Keys that may sign a SAML assertion.
    pub saml_keys: Vec<Jwk>,
    /// Where this provider's records are governed: `ZW`, `EU`, `US`. Kept on the
    /// session so downstream services know which rulebook applies.
    pub region: String,
    pub default_locale: Locale,
    pub session: SessionPolicy,
    pub enabled: bool,
}

impl IdpConfig {
    /// A minimal, safe registration: asymmetric signing only, email verified,
    /// eight-hour absolute sessions with a thirty-minute idle window.
    pub fn new(
        organization_id: &str,
        alias: &str,
        kind: ProviderKind,
        issuer: &str,
        audience: &str,
    ) -> IdpConfig {
        IdpConfig {
            organization_id: organization_id.to_string(),
            alias: alias.to_string(),
            kind,
            issuer: issuer.to_string(),
            audiences: vec![audience.to_string()],
            client_id: None,
            jwks: Jwks::default(),
            discovery: None,
            claims: ClaimsPolicy {
                issuer: issuer.to_string(),
                audiences: vec![audience.to_string()],
                ..ClaimsPolicy::default()
            },
            saml_keys: Vec::new(),
            region: "ZW".to_string(),
            default_locale: Locale::new("en", Some("ZW")),
            session: SessionPolicy::default(),
            enabled: true,
        }
    }

    pub fn with_jwks(mut self, jwks: Jwks) -> IdpConfig {
        self.jwks = jwks;
        self
    }

    pub fn with_saml_key(mut self, key: Jwk) -> IdpConfig {
        self.saml_keys.push(key);
        self
    }

    pub fn with_region(mut self, region: &str) -> IdpConfig {
        self.region = region.to_ascii_uppercase();
        self
    }

    pub fn with_locale(mut self, locale: Locale) -> IdpConfig {
        self.default_locale = locale;
        self
    }

    pub fn with_algorithms(mut self, policy: AlgPolicy) -> IdpConfig {
        self.claims.algorithms = policy;
        self
    }

    pub fn with_email_domains(mut self, domains: &[&str]) -> IdpConfig {
        self.claims.allowed_email_domains = domains.iter().map(|d| d.to_string()).collect();
        self
    }

    pub fn with_group_roles(mut self, pairs: &[(&str, &str)]) -> IdpConfig {
        self.claims.group_roles = pairs
            .iter()
            .map(|(group, role)| (group.to_string(), role.to_string()))
            .collect();
        self
    }

    /// The claim policy this provider will be verified against, with its own
    /// issuer and audience list.
    pub fn claims_policy(&self) -> ClaimsPolicy {
        let mut policy = self.claims.clone();
        policy.issuer = self.issuer.clone();
        if policy.audiences.is_empty() {
            policy.audiences = self.audiences.clone();
        }
        policy
    }

    /// The token nonce binding is only meaningful for interactive sign-in, so
    /// the caller decides when to require it.
    pub fn claims_policy_with_nonce(&self, nonce: Option<&str>) -> ClaimsPolicy {
        let mut policy = self.claims_policy();
        policy.expected_nonce = nonce.map(ToString::to_string);
        policy
    }
}

#[derive(Debug, Clone, Default)]
pub struct IdpRegistry {
    entries: IndexMap<String, IdpConfig>,
}

fn key(organization_id: &str, alias: &str) -> String {
    format!("{organization_id}/{alias}")
}

impl IdpRegistry {
    pub fn new() -> Self {
        IdpRegistry::default()
    }

    /// Register or replace. Replacing is how a key rotation is rolled out.
    pub fn register(&mut self, config: IdpConfig) {
        self.entries
            .insert(key(&config.organization_id, &config.alias), config);
    }

    pub fn get(&self, organization_id: &str, alias: &str) -> Result<&IdpConfig, SsoError> {
        let config = self
            .entries
            .get(&key(organization_id, alias))
            .ok_or_else(|| SsoError::with_detail(MessageKey::IdpNotConfigured, alias))?;
        if !config.enabled {
            return Err(SsoError::with_detail(MessageKey::IdpDisabled, alias));
        }
        Ok(config)
    }

    pub fn get_mut(&mut self, organization_id: &str, alias: &str) -> Option<&mut IdpConfig> {
        self.entries.get_mut(&key(organization_id, alias))
    }

    pub fn set_enabled(&mut self, organization_id: &str, alias: &str, enabled: bool) -> bool {
        match self.get_mut(organization_id, alias) {
            Some(config) => {
                config.enabled = enabled;
                true
            }
            None => false,
        }
    }

    /// The providers one organisation can choose between, disabled ones
    /// included so an admin screen can show them.
    pub fn for_organization(&self, organization_id: &str) -> Vec<&IdpConfig> {
        self.entries
            .values()
            .filter(|config| config.organization_id == organization_id)
            .collect()
    }

    /// What a login page shows: alias, kind and the organisation's language.
    pub fn advertised(&self, organization_id: &str) -> Vec<serde_json::Value> {
        self.for_organization(organization_id)
            .into_iter()
            .filter(|config| config.enabled)
            .map(|config| {
                serde_json::json!({
                    "alias": config.alias,
                    "provider": config.kind.value(),
                    "locale": config.default_locale.to_string(),
                    "region": config.region,
                })
            })
            .collect()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}
