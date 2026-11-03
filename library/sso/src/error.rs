//! Rejections: a stable code, an HTTP status, and a message in the caller's
//! language.

use serde_json::{json, Value};

use crate::i18n::{Locale, MessageCatalogue, MessageKey};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SsoError {
    pub key: MessageKey,
    /// Details for the log: the offending issuer, audience, domain, algorithm.
    /// Never a secret, never the token itself.
    pub detail: Option<String>,
}

impl SsoError {
    pub fn new(key: MessageKey) -> SsoError {
        SsoError { key, detail: None }
    }

    pub fn with_detail(key: MessageKey, detail: impl Into<String>) -> SsoError {
        SsoError {
            key,
            detail: Some(detail.into()),
        }
    }

    pub fn code(&self) -> &'static str {
        self.key.code()
    }

    pub fn status_code(&self) -> u16 {
        use MessageKey::*;
        match self.key {
            UnsupportedProvider | UnsupportedAlgorithm | IdpNotConfigured | IdpDisabled => 400,
            InvalidSession | SessionExpired | SessionIdle | SessionIpMismatch => 401,
            EmailDomainNotAllowed => 403,
            _ => 401,
        }
    }

    /// The English wording, which is what the original service always sent.
    pub fn message(&self) -> String {
        self.localized(&Locale::new(Locale::ENGLISH, None))
    }

    pub fn localized(&self, locale: &Locale) -> String {
        let base = MessageCatalogue::render(self.key, locale);
        match (&self.detail, self.key) {
            (Some(detail), MessageKey::UnsupportedProvider) => format!("{base}: {detail}"),
            (Some(detail), MessageKey::UnsupportedAlgorithm) => format!("{base}: {detail}"),
            (Some(detail), _) => format!("{base} ({detail})"),
            (None, _) => base,
        }
    }

    /// The body an HTTP layer can return directly.
    pub fn to_json(&self, locale: &Locale) -> Value {
        json!({
            "detail": self.localized(locale),
            "code": self.code(),
        })
    }
}

impl std::fmt::Display for SsoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code(), self.message())
    }
}

impl std::error::Error for SsoError {}
