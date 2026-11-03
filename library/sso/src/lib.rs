//! Vimbai enterprise SSO as a library.
//!
//! The FastAPI service kept its session store in a module global with no tenant
//! dimension, so a token from any caller was visible to every caller. Here the
//! caller owns the store, instants are parameters rather than clock reads, and
//! the token derivation takes a nonce, so sessions are reproducible in tests.
//!
//! What is preserved: the 401/400 contract and wording, the eight-hour session
//! lifetime, the ten-character IdP token floor, the twenty-character SAML floor,
//! and the fact that `/config` stores nothing.
//!
//! What is deliberately not preserved: signature verification. The service never
//! verified a JWT signature or a SAML signature, and neither does this, so a
//! caller must validate the assertion with the identity provider first.

#![forbid(unsafe_code)]

pub mod clock;
pub mod crypto;
pub mod error;
pub mod provider;
pub mod store;

pub use error::SsoError;
pub use provider::Provider;
pub use store::{
    validate_oidc_token, validate_saml_assertion, AuthOutcome, Session, SsoAuthRequest, SsoStore,
    SESSION_TTL_MICROSECONDS,
};

#[cfg(test)]
mod tests;
