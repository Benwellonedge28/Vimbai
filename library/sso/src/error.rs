//! The rejections the service returned, with its status codes and wording.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SsoError {
    /// 401 "Invalid IdP token" - missing or shorter than ten characters.
    InvalidIdpToken,
    /// 401 "Malformed token" - not a three-part JWT.
    MalformedToken,
    /// 401 "Invalid token payload" - the middle segment would not decode.
    InvalidTokenPayload,
    /// 401 "Token expired" - the `exp` claim is in the past.
    TokenExpired,
    /// 401 "Invalid SAML assertion" - shorter than twenty characters.
    InvalidSamlAssertion,
    /// 400 "Unsupported provider: x".
    UnsupportedProvider(String),
    /// 401 "Invalid or expired session" - unknown token.
    InvalidSession,
    /// 401 "Session expired" - recognised, past its expiry, and dropped.
    SessionExpired,
}

impl SsoError {
    pub fn status_code(&self) -> u16 {
        match self {
            SsoError::UnsupportedProvider(_) => 400,
            _ => 401,
        }
    }

    pub fn message(&self) -> String {
        match self {
            SsoError::InvalidIdpToken => "Invalid IdP token".to_string(),
            SsoError::MalformedToken => "Malformed token".to_string(),
            SsoError::InvalidTokenPayload => "Invalid token payload".to_string(),
            SsoError::TokenExpired => "Token expired".to_string(),
            SsoError::InvalidSamlAssertion => "Invalid SAML assertion".to_string(),
            SsoError::UnsupportedProvider(provider) => {
                format!("Unsupported provider: {provider}")
            }
            SsoError::InvalidSession => "Invalid or expired session".to_string(),
            SsoError::SessionExpired => "Session expired".to_string(),
        }
    }
}
