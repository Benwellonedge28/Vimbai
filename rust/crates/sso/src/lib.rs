//! Enterprise single sign-on for Vimbai.
//!
//! Verification first: an identity provider token is checked against a published
//! key and an algorithm policy, then its claims are held against the
//! organisation's rules. What arrives is either an `Identity` or a rejection
//! carrying a stable code and a message in the caller's language.
//!
//! Deliberate boundaries, so no caller over-trusts this crate:
//! * The caller fetches the discovery document and the key set. Nothing here
//!   performs HTTP, so a token cannot be verified against a URL an attacker
//!   supplied mid-request.
//! * A SAML assertion is verified over the exact bytes received. An enveloped
//!   signature inside a larger document needs canonicalisation, which this does
//!   not implement; supply the assertion element.
//! * ES384, ES512, EdDSA are refused rather than half-supported.

#![forbid(unsafe_code)]

pub mod claims;
pub mod clock;
pub mod crypto;
pub mod discovery;
pub mod error;
pub mod i18n;
pub mod idp;
pub mod jose;
pub mod saml;
pub mod session;
pub mod store;

#[cfg(test)]
mod fixtures;
#[cfg(test)]
mod tests;

pub use claims::{verify_and_map, verify_with_key, ClaimsPolicy, Identity};
pub use clock::{parse_iso, to_iso};
pub use discovery::DiscoveryDocument;
pub use error::SsoError;
pub use i18n::{accept_language, negotiate, Locale, MessageCatalogue, MessageKey};
pub use idp::{IdpConfig, IdpRegistry, ProviderKind};
pub use jose::{verify_token, Alg, AlgPolicy, Jwk, Jwks, ParsedJwt};
pub use saml::{parse_assertion, SamlAssertion};
pub use session::{AuditKind, Session, SessionPolicy, SessionStore};
pub use store::{AuthOutcome, AuthRequest, Credential, SsoStore};
