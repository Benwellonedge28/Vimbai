//! Turning verified claims into an identity, and refusing the ones that should
//! not be trusted.

use serde_json::Value;

use crate::error::SsoError;
use crate::i18n::Locale;
use crate::jose::{Alg, AlgPolicy};
use crate::jose::{Jwks, ParsedJwt};

/// Who is allowed to log in, and what they become.
#[derive(Debug, Clone, Default)]
pub struct ClaimsPolicy {
    pub issuer: String,
    /// Any of these audiences satisfies the token.
    pub audiences: Vec<String>,
    /// Accepted clock difference between us and the provider.
    pub leeway_seconds: i64,
    /// When set, the token must carry this nonce: proof it answers our request.
    pub expected_nonce: Option<String>,
    /// Empty means any domain.
    pub allowed_email_domains: Vec<String>,
    pub require_verified_email: bool,
    /// IdP group name to Vimbai role. Unmapped groups are ignored.
    pub group_roles: Vec<(String, String)>,
    pub algorithms: AlgPolicy,
}

/// The authenticated person, as Vimbai will treat them.
#[derive(Debug, Clone, PartialEq)]
pub struct Identity {
    pub user_id: String,
    pub email: Option<String>,
    pub roles: Vec<String>,
    pub groups: Vec<String>,
    /// The person's own locale and zone, when the provider sent them. A login
    /// from Harare keeps Harare's language and clock without anyone asking.
    pub locale: Option<Locale>,
    pub timezone: Option<String>,
    pub display_name: Option<String>,
}

fn string_claim(claims: &Value, name: &str) -> Option<String> {
    claims
        .get(name)
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

fn audience_list(claims: &Value) -> Vec<String> {
    match claims.get("aud") {
        Some(Value::String(single)) => vec![single.clone()],
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(Value::as_str)
            .map(ToString::to_string)
            .collect(),
        _ => Vec::new(),
    }
}

fn number_claim(claims: &Value, name: &str) -> Option<i64> {
    match claims.get(name) {
        Some(Value::Number(number)) => number
            .as_i64()
            .or_else(|| number.as_f64().map(|f| f as i64)),
        Some(Value::String(text)) => text.parse().ok(),
        _ => None,
    }
}

/// Full verification: signature, then every claim the policy cares about.
pub fn verify_and_map(
    token: &str,
    jwks: &Jwks,
    policy: &ClaimsPolicy,
    now_micros: i64,
) -> Result<Identity, SsoError> {
    let parsed = crate::jose::verify_token(token, jwks, &policy.algorithms)?;
    map_claims(&parsed, policy, now_micros, Some(jwks))
}

/// Verify against an inline key (a single-key provider), then map the claims.
pub fn verify_with_key(
    token: &str,
    key: &crate::jose::Jwk,
    policy: &ClaimsPolicy,
    now_micros: i64,
) -> Result<Identity, SsoError> {
    let parsed = ParsedJwt::parse(token)?;
    let alg: Alg = parsed.alg()?;
    if !policy.algorithms.allows(alg) {
        return Err(SsoError::with_detail(
            MessageKey::UnsupportedAlgorithm,
            alg.name(),
        ));
    }
    parsed.verify(alg, key)?;
    map_claims(&parsed, policy, now_micros, None)
}

use crate::i18n::MessageKey;

/// Claims mapping for a token whose signature is already trusted.
pub fn map_claims(
    parsed: &ParsedJwt,
    policy: &ClaimsPolicy,
    now_micros: i64,
    _jwks: Option<&Jwks>,
) -> Result<Identity, SsoError> {
    let claims = &parsed.claims;
    let now_seconds = now_micros as f64 / 1_000_000.0;
    let leeway = policy.leeway_seconds as f64;

    let issuer = string_claim(claims, "iss")
        .ok_or_else(|| SsoError::with_detail(MessageKey::MissingClaim, "iss"))?;
    if issuer.trim_end_matches('/') != policy.issuer.trim_end_matches('/') {
        return Err(SsoError::with_detail(MessageKey::IssuerMismatch, issuer));
    }

    let audiences = audience_list(claims);
    if policy.audiences.is_empty() {
        return Err(SsoError::with_detail(MessageKey::MissingClaim, "aud"));
    }
    if !audiences.iter().any(|aud| policy.audiences.contains(aud)) {
        return Err(SsoError::with_detail(
            MessageKey::AudienceMismatch,
            audiences.join(","),
        ));
    }

    let exp = number_claim(claims, "exp")
        .ok_or_else(|| SsoError::with_detail(MessageKey::MissingClaim, "exp"))?;
    if (exp as f64) + leeway < now_seconds {
        return Err(SsoError::new(MessageKey::TokenExpired));
    }
    if let Some(nbf) = number_claim(claims, "nbf") {
        if (nbf as f64) - leeway > now_seconds {
            return Err(SsoError::new(MessageKey::TokenNotYetValid));
        }
    }
    if let Some(iat) = number_claim(claims, "iat") {
        // A token minted far in the future is a replay waiting to happen.
        if (iat as f64) - leeway > now_seconds {
            return Err(SsoError::with_detail(MessageKey::TokenNotYetValid, "iat"));
        }
    }

    if let Some(expected) = &policy.expected_nonce {
        match string_claim(claims, "nonce") {
            Some(found) if &found == expected => {}
            Some(_) => return Err(SsoError::new(MessageKey::NonceMismatch)),
            None => return Err(SsoError::with_detail(MessageKey::MissingClaim, "nonce")),
        }
    }

    let user_id = string_claim(claims, "sub")
        .ok_or_else(|| SsoError::with_detail(MessageKey::MissingClaim, "sub"))?;
    let email = string_claim(claims, "email");
    if policy.require_verified_email {
        let verified = claims
            .get("email_verified")
            .and_then(|v| v.as_bool().or_else(|| v.as_str().map(|s| s == "true")))
            .unwrap_or(false);
        if !verified {
            return Err(SsoError::new(MessageKey::EmailNotVerified));
        }
    }
    if !policy.allowed_email_domains.is_empty() {
        let domain_ok = email
            .as_deref()
            .and_then(|address| address.rsplit_once('@').map(|(_, domain)| domain))
            .map(|domain| {
                let domain = domain.to_ascii_lowercase();
                policy
                    .allowed_email_domains
                    .iter()
                    .any(|allowed| allowed.eq_ignore_ascii_case(&domain))
            })
            .unwrap_or(false);
        if !domain_ok {
            return Err(SsoError::with_detail(
                MessageKey::EmailDomainNotAllowed,
                email.clone().unwrap_or_default(),
            ));
        }
    }

    let groups: Vec<String> = match claims.get("groups") {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(Value::as_str)
            .map(ToString::to_string)
            .collect(),
        Some(Value::String(joined)) => joined
            .split([',', ' '])
            .filter(|part| !part.is_empty())
            .map(ToString::to_string)
            .collect(),
        _ => Vec::new(),
    };
    let mut roles: Vec<String> = policy
        .group_roles
        .iter()
        .filter(|(group, _)| groups.iter().any(|held| held == group))
        .map(|(_, role)| role.clone())
        .collect();
    // A provider that assigns roles directly is honoured too, with mapping still
    // applied so an organisation can rename them.
    if let Some(direct) = claims.get("roles").and_then(Value::as_array) {
        for role in direct.iter().filter_map(Value::as_str) {
            let mapped = policy
                .group_roles
                .iter()
                .find(|(group, _)| group == role)
                .map(|(_, role)| role.clone())
                .unwrap_or_else(|| role.to_string());
            roles.push(mapped);
        }
    }
    roles.sort();
    roles.dedup();

    let locale = claims
        .get("locale")
        .and_then(Value::as_str)
        .and_then(Locale::parse);

    Ok(Identity {
        user_id,
        email,
        roles,
        groups,
        locale,
        timezone: string_claim(claims, "zoneinfo"),
        display_name: string_claim(claims, "name"),
    })
}
