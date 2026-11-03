//! Locale negotiation and the message catalogue.
//!
//! The service returned English strings with no way to ask for another
//! language. Here every rejection carries a stable machine code plus a message
//! in the caller's language, negotiated from `Accept-Language` or taken from the
//! organisation's default locale.

use std::fmt;

use serde::{Deserialize, Serialize};

/// A BCP-47 tag, narrowed to the parts the catalogue actually distinguishes.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Locale {
    pub language: String,
    pub region: Option<String>,
}

impl Locale {
    pub fn new(language: &str, region: Option<&str>) -> Locale {
        Locale {
            language: language.to_ascii_lowercase(),
            region: region.map(|r| r.to_ascii_uppercase()),
        }
    }

    pub const ENGLISH: &'static str = "en";

    /// Parse `sn-ZW`, `pt_BR`, `en`, or `zh-Hans-CN`. Script and variant
    /// subtags are dropped: the catalogue is keyed by language and region.
    pub fn parse(tag: &str) -> Option<Locale> {
        let cleaned = tag.trim().replace('_', "-");
        let mut parts = cleaned.split('-').filter(|p| !p.is_empty());
        let language = parts.next()?.to_ascii_lowercase();
        if !language.chars().all(|c| c.is_ascii_alphabetic()) || language.len() > 8 {
            return None;
        }
        let region = parts
            .find(|part| part.len() == 2 && part.chars().all(|c| c.is_ascii_alphabetic()))
            .map(|part| part.to_ascii_uppercase());
        Some(Locale { language, region })
    }

    /// The fallback ladder: `sn-ZW` -> `sn` -> `en`.
    pub fn chain(&self) -> Vec<Locale> {
        let mut chain = vec![self.clone()];
        if self.region.is_some() {
            chain.push(Locale::new(&self.language, None));
        }
        let english = Locale::new(Locale::ENGLISH, None);
        if chain.last() != Some(&english) {
            chain.push(english);
        }
        chain
    }
}

impl fmt::Display for Locale {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.region {
            Some(region) => write!(f, "{}-{region}", self.language),
            None => write!(f, "{}", self.language),
        }
    }
}

/// One entry in `Accept-Language`, in the order the client ranked it.
pub fn accept_language(header: &str) -> Vec<Locale> {
    let mut ranked: Vec<(f64, usize, Locale)> = header
        .split(',')
        .enumerate()
        .filter_map(|(position, part)| {
            let mut pieces = part.split(';');
            let tag = pieces.next()?.trim();
            if tag == "*" || tag.is_empty() {
                return None;
            }
            let quality = pieces
                .find_map(|piece| piece.trim().strip_prefix("q="))
                .and_then(|value| value.parse::<f64>().ok())
                .unwrap_or(1.0);
            if quality <= 0.0 {
                return None;
            }
            Locale::parse(tag).map(|locale| (quality, position, locale))
        })
        .collect();
    ranked.sort_by(|a, b| {
        b.0.partial_cmp(&a.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.1.cmp(&b.1))
    });
    ranked.into_iter().map(|(_, _, locale)| locale).collect()
}

/// Pick the best supported locale for a request.
pub fn negotiate(header: Option<&str>, organization_default: Option<&Locale>) -> Locale {
    let supported = MessageCatalogue::supported();
    if let Some(header) = header {
        for requested in accept_language(header) {
            for candidate in requested.chain() {
                if supported.contains(&candidate.language) {
                    return candidate;
                }
            }
        }
    }
    if let Some(default) = organization_default {
        for candidate in default.chain() {
            if supported.contains(&candidate.language) {
                return candidate;
            }
        }
    }
    Locale::new(Locale::ENGLISH, None)
}

/// The messages the crate can return, identified by a stable code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MessageKey {
    InvalidIdpToken,
    MalformedToken,
    InvalidTokenPayload,
    TokenExpired,
    TokenNotYetValid,
    InvalidSamlAssertion,
    UnsupportedProvider,
    UnsupportedAlgorithm,
    SignatureInvalid,
    UnknownSigningKey,
    IssuerMismatch,
    AudienceMismatch,
    NonceMismatch,
    MissingClaim,
    EmailNotVerified,
    EmailDomainNotAllowed,
    AssertionConditionsFailed,
    AudienceRestrictionFailed,
    IdpNotConfigured,
    IdpDisabled,
    DiscoveryInvalid,
    InvalidSession,
    SessionExpired,
    SessionIdle,
    SessionIpMismatch,
    ConcurrentSessionEvicted,
}

impl MessageKey {
    /// The stable machine-readable code, never translated.
    pub fn code(self) -> &'static str {
        match self {
            MessageKey::InvalidIdpToken => "invalid_idp_token",
            MessageKey::MalformedToken => "malformed_token",
            MessageKey::InvalidTokenPayload => "invalid_token_payload",
            MessageKey::TokenExpired => "token_expired",
            MessageKey::TokenNotYetValid => "token_not_yet_valid",
            MessageKey::InvalidSamlAssertion => "invalid_saml_assertion",
            MessageKey::UnsupportedProvider => "unsupported_provider",
            MessageKey::UnsupportedAlgorithm => "unsupported_algorithm",
            MessageKey::SignatureInvalid => "signature_invalid",
            MessageKey::UnknownSigningKey => "unknown_signing_key",
            MessageKey::IssuerMismatch => "issuer_mismatch",
            MessageKey::AudienceMismatch => "audience_mismatch",
            MessageKey::NonceMismatch => "nonce_mismatch",
            MessageKey::MissingClaim => "missing_claim",
            MessageKey::EmailNotVerified => "email_not_verified",
            MessageKey::EmailDomainNotAllowed => "email_domain_not_allowed",
            MessageKey::AssertionConditionsFailed => "assertion_conditions_failed",
            MessageKey::AudienceRestrictionFailed => "audience_restriction_failed",
            MessageKey::IdpNotConfigured => "idp_not_configured",
            MessageKey::IdpDisabled => "idp_disabled",
            MessageKey::DiscoveryInvalid => "discovery_invalid",
            MessageKey::InvalidSession => "invalid_session",
            MessageKey::SessionExpired => "session_expired",
            MessageKey::SessionIdle => "session_idle",
            MessageKey::SessionIpMismatch => "session_ip_mismatch",
            MessageKey::ConcurrentSessionEvicted => "concurrent_session_evicted",
        }
    }
}

pub struct MessageCatalogue;

impl MessageCatalogue {
    pub fn supported() -> Vec<String> {
        ["en", "sn", "fr", "pt", "es", "de", "ar", "zh"]
            .iter()
            .map(ToString::to_string)
            .collect()
    }

    /// English is always the final fallback, so this never fails.
    pub fn render(key: MessageKey, locale: &Locale) -> String {
        let language = locale.language.as_str();
        match (key, language) {
            (MessageKey::InvalidIdpToken, "sn") => "Chitupa cheIdP hachishandi".to_string(),
            (MessageKey::MalformedToken, "sn") => "Chitupa chakasvibiswa".to_string(),
            (MessageKey::TokenExpired, "sn") => "Chitupa chapera nguva".to_string(),
            (MessageKey::InvalidSession, "sn") => "Musangano haushandi kana wapera".to_string(),
            (MessageKey::SessionExpired, "sn") => "Musangano wapera".to_string(),

            (MessageKey::InvalidIdpToken, "fr") => "Jeton IdP invalide".to_string(),
            (MessageKey::MalformedToken, "fr") => "Jeton malformé".to_string(),
            (MessageKey::InvalidTokenPayload, "fr") => "Charge utile du jeton invalide".to_string(),
            (MessageKey::TokenExpired, "fr") => "Jeton expiré".to_string(),
            (MessageKey::TokenNotYetValid, "fr") => "Jeton pas encore valide".to_string(),
            (MessageKey::InvalidSamlAssertion, "fr") => "Assertion SAML invalide".to_string(),
            (MessageKey::UnsupportedProvider, "fr") => "Fournisseur non pris en charge".to_string(),
            (MessageKey::SignatureInvalid, "fr") => "Signature invalide".to_string(),
            (MessageKey::AudienceMismatch, "fr") => "Audience non concordante".to_string(),
            (MessageKey::IssuerMismatch, "fr") => "Émetteur non concordant".to_string(),
            (MessageKey::InvalidSession, "fr") => "Session invalide ou expirée".to_string(),
            (MessageKey::SessionExpired, "fr") => "Session expirée".to_string(),

            (MessageKey::InvalidIdpToken, "pt") => "Token do IdP inválido".to_string(),
            (MessageKey::MalformedToken, "pt") => "Token malformado".to_string(),
            (MessageKey::TokenExpired, "pt") => "Token expirado".to_string(),
            (MessageKey::SignatureInvalid, "pt") => "Assinatura inválida".to_string(),
            (MessageKey::InvalidSession, "pt") => "Sessão inválida ou expirada".to_string(),
            (MessageKey::SessionExpired, "pt") => "Sessão expirada".to_string(),

            (MessageKey::InvalidIdpToken, "es") => "Token del IdP no válido".to_string(),
            (MessageKey::MalformedToken, "es") => "Token mal formado".to_string(),
            (MessageKey::TokenExpired, "es") => "Token caducado".to_string(),
            (MessageKey::SignatureInvalid, "es") => "Firma no válida".to_string(),
            (MessageKey::InvalidSession, "es") => "Sesión no válida o caducada".to_string(),
            (MessageKey::SessionExpired, "es") => "Sesión caducada".to_string(),

            (MessageKey::InvalidIdpToken, "de") => "Ungültiges IdP-Token".to_string(),
            (MessageKey::MalformedToken, "de") => "Fehlerhaftes Token".to_string(),
            (MessageKey::TokenExpired, "de") => "Token abgelaufen".to_string(),
            (MessageKey::SignatureInvalid, "de") => "Ungültige Signatur".to_string(),
            (MessageKey::InvalidSession, "de") => "Sitzung ungültig oder abgelaufen".to_string(),
            (MessageKey::SessionExpired, "de") => "Sitzung abgelaufen".to_string(),

            (MessageKey::InvalidIdpToken, "ar") => "رمز IdP غير صالح".to_string(),
            (MessageKey::MalformedToken, "ar") => "رمز مشوّه".to_string(),
            (MessageKey::TokenExpired, "ar") => "انتهت صلاحية الرمز".to_string(),
            (MessageKey::SignatureInvalid, "ar") => "توقيع غير صالح".to_string(),
            (MessageKey::InvalidSession, "ar") => "جلسة غير صالحة أو منتهية".to_string(),
            (MessageKey::SessionExpired, "ar") => "انتهت الجلسة".to_string(),

            (MessageKey::InvalidIdpToken, "zh") => "IdP 令牌无效".to_string(),
            (MessageKey::MalformedToken, "zh") => "令牌格式错误".to_string(),
            (MessageKey::TokenExpired, "zh") => "令牌已过期".to_string(),
            (MessageKey::SignatureInvalid, "zh") => "签名无效".to_string(),
            (MessageKey::InvalidSession, "zh") => "会话无效或已过期".to_string(),
            (MessageKey::SessionExpired, "zh") => "会话已过期".to_string(),

            _ => english(key).to_string(),
        }
    }
}

/// The English wording, which the original service used for every case.
pub fn english(key: MessageKey) -> &'static str {
    match key {
        MessageKey::InvalidIdpToken => "Invalid IdP token",
        MessageKey::MalformedToken => "Malformed token",
        MessageKey::InvalidTokenPayload => "Invalid token payload",
        MessageKey::TokenExpired => "Token expired",
        MessageKey::TokenNotYetValid => "Token is not yet valid",
        MessageKey::InvalidSamlAssertion => "Invalid SAML assertion",
        MessageKey::UnsupportedProvider => "Unsupported provider",
        MessageKey::UnsupportedAlgorithm => "Unsupported signing algorithm",
        MessageKey::SignatureInvalid => "Signature verification failed",
        MessageKey::UnknownSigningKey => "Unknown signing key",
        MessageKey::IssuerMismatch => "Issuer does not match the configured identity provider",
        MessageKey::AudienceMismatch => "Token audience does not include this application",
        MessageKey::NonceMismatch => "Nonce does not match the login attempt",
        MessageKey::MissingClaim => "Required claim is missing",
        MessageKey::EmailNotVerified => "Email address is not verified by the identity provider",
        MessageKey::EmailDomainNotAllowed => "Email domain is not permitted for this organisation",
        MessageKey::AssertionConditionsFailed => "Assertion is outside its validity window",
        MessageKey::AudienceRestrictionFailed => "Assertion is not addressed to this application",
        MessageKey::IdpNotConfigured => "No identity provider is configured under that name",
        MessageKey::IdpDisabled => "Identity provider is disabled",
        MessageKey::DiscoveryInvalid => "OpenID Connect discovery document is invalid",
        MessageKey::InvalidSession => "Invalid or expired session",
        MessageKey::SessionExpired => "Session expired",
        MessageKey::SessionIdle => "Session ended after inactivity",
        MessageKey::SessionIpMismatch => "Session is bound to a different address",
        MessageKey::ConcurrentSessionEvicted => "An older session was ended by the session limit",
    }
}
