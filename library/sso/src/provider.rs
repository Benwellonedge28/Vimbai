//! The identity providers the service accepted.

use serde::{Deserialize, Serialize};

pub const RESPONSE_PROVIDER_ORDER: [Provider; 4] = [
    Provider::Oidc,
    Provider::Saml,
    Provider::AzureAd,
    Provider::Okta,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    Oidc,
    Saml,
    #[serde(rename = "azure_ad")]
    AzureAd,
    Okta,
}

impl Provider {
    pub fn value(self) -> &'static str {
        match self {
            Provider::Oidc => "oidc",
            Provider::Saml => "saml",
            Provider::AzureAd => "azure_ad",
            Provider::Okta => "okta",
        }
    }

    pub fn parse(value: &str) -> Option<Provider> {
        RESPONSE_PROVIDER_ORDER
            .iter()
            .copied()
            .find(|provider| provider.value() == value)
    }

    /// OIDC-style providers hand over a JWT; SAML hands over an assertion.
    pub fn speaks_jwt(self) -> bool {
        matches!(self, Provider::Oidc | Provider::AzureAd | Provider::Okta)
    }
}
