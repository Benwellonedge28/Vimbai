//! JOSE: parsing an identity provider's token and verifying its signature.
//!
//! The service only decoded the middle segment and trusted whoever sent it. This
//! module verifies the signature against a published key, refuses `alg: none`
//! and refuses any algorithm the organisation has not allowed, which is the
//! class of bug that lets a public key be replayed as an HMAC secret.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use hmac::{Hmac, Mac};
use p256::ecdsa::{Signature as EcSignature, VerifyingKey as EcKey};
use p256::EncodedPoint;
use rsa::pkcs1v15::Pkcs1v15Sign;
use rsa::{BigUint, Pss, RsaPublicKey};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256, Sha384, Sha512};
use subtle::ConstantTimeEq;

use crate::error::SsoError;
use crate::i18n::MessageKey;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Alg {
    Hs256,
    Hs384,
    Hs512,
    Rs256,
    Rs384,
    Rs512,
    Ps256,
    Ps384,
    Ps512,
    Es256,
}

impl Alg {
    pub fn name(self) -> &'static str {
        match self {
            Alg::Hs256 => "HS256",
            Alg::Hs384 => "HS384",
            Alg::Hs512 => "HS512",
            Alg::Rs256 => "RS256",
            Alg::Rs384 => "RS384",
            Alg::Rs512 => "RS512",
            Alg::Ps256 => "PS256",
            Alg::Ps384 => "PS384",
            Alg::Ps512 => "PS512",
            Alg::Es256 => "ES256",
        }
    }

    /// `none` is not an algorithm here: it never parses, so it can never verify.
    pub fn parse(name: &str) -> Option<Alg> {
        match name {
            "HS256" => Some(Alg::Hs256),
            "HS384" => Some(Alg::Hs384),
            "HS512" => Some(Alg::Hs512),
            "RS256" => Some(Alg::Rs256),
            "RS384" => Some(Alg::Rs384),
            "RS512" => Some(Alg::Rs512),
            "PS256" => Some(Alg::Ps256),
            "PS384" => Some(Alg::Ps384),
            "PS512" => Some(Alg::Ps512),
            "ES256" => Some(Alg::Es256),
            _ => None,
        }
    }

    /// Shared-secret algorithms are only sane for a client we trust with the
    /// secret; a federated identity provider must sign asymmetrically.
    pub fn is_symmetric(self) -> bool {
        matches!(self, Alg::Hs256 | Alg::Hs384 | Alg::Hs512)
    }
}

/// What an organisation will accept. Asymmetric defaults, because that is what
/// real identity providers use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlgPolicy {
    pub allowed: Vec<Alg>,
}

impl Default for AlgPolicy {
    fn default() -> Self {
        AlgPolicy {
            allowed: vec![
                Alg::Rs256,
                Alg::Ps256,
                Alg::Es256,
                Alg::Rs384,
                Alg::Rs512,
                Alg::Ps384,
                Alg::Ps512,
            ],
        }
    }
}

impl AlgPolicy {
    pub fn permitting(allowed: Vec<Alg>) -> Self {
        AlgPolicy { allowed }
    }

    pub fn allows(&self, alg: Alg) -> bool {
        self.allowed.contains(&alg)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Header {
    pub alg: String,
    #[serde(default)]
    pub kid: Option<String>,
    #[serde(default)]
    pub typ: Option<String>,
}

/// A token whose signature has not yet been checked.
#[derive(Debug, Clone)]
pub struct ParsedJwt {
    pub header: Header,
    pub claims: Value,
    signing_input: Vec<u8>,
    signature: Vec<u8>,
}

impl ParsedJwt {
    pub fn parse(token: &str) -> Result<ParsedJwt, SsoError> {
        let segments: Vec<&str> = token.split('.').collect();
        if segments.len() != 3 {
            return Err(SsoError::new(MessageKey::MalformedToken));
        }
        let header_bytes = URL_SAFE_NO_PAD
            .decode(segments[0])
            .map_err(|_| SsoError::new(MessageKey::MalformedToken))?;
        let header: Header = serde_json::from_slice(&header_bytes)
            .map_err(|_| SsoError::new(MessageKey::MalformedToken))?;
        let payload_bytes = URL_SAFE_NO_PAD
            .decode(segments[1])
            .map_err(|_| SsoError::new(MessageKey::InvalidTokenPayload))?;
        let claims: Value = serde_json::from_slice(&payload_bytes)
            .map_err(|_| SsoError::new(MessageKey::InvalidTokenPayload))?;
        let signature = URL_SAFE_NO_PAD
            .decode(segments[2])
            .map_err(|_| SsoError::new(MessageKey::SignatureInvalid))?;
        let signing_input = format!("{}.{}", segments[0], segments[1]).into_bytes();
        Ok(ParsedJwt {
            header,
            claims,
            signing_input,
            signature,
        })
    }

    pub fn alg(&self) -> Result<Alg, SsoError> {
        Alg::parse(&self.header.alg).ok_or_else(|| {
            SsoError::with_detail(MessageKey::UnsupportedAlgorithm, &self.header.alg)
        })
    }

    pub fn kid(&self) -> Option<&str> {
        self.header.kid.as_deref()
    }

    /// Check the signature with a published key.
    pub fn verify(&self, alg: Alg, key: &Jwk) -> Result<(), SsoError> {
        if self.header.alg != alg.name() {
            return Err(SsoError::with_detail(
                MessageKey::SignatureInvalid,
                format!(
                    "header says {}, policy says {}",
                    self.header.alg,
                    alg.name()
                ),
            ));
        }
        key.verify(alg, &self.signing_input, &self.signature)
    }
}

/// A public key as a JWKS publishes it.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Jwk {
    pub kty: String,
    #[serde(default)]
    pub kid: Option<String>,
    #[serde(default)]
    pub alg: Option<String>,
    #[serde(default)]
    pub r#use: Option<String>,
    #[serde(default)]
    pub n: Option<String>,
    #[serde(default)]
    pub e: Option<String>,
    #[serde(default)]
    pub crv: Option<String>,
    #[serde(default)]
    pub x: Option<String>,
    #[serde(default)]
    pub y: Option<String>,
    /// An HMAC secret, base64url. Only for a trusted first-party client.
    #[serde(default)]
    pub k: Option<String>,
}

impl Jwk {
    pub fn verify(&self, alg: Alg, signing_input: &[u8], signature: &[u8]) -> Result<(), SsoError> {
        match self.kty.as_str() {
            "RSA" => verify_rsa(self, alg, signing_input, signature),
            "EC" => verify_ec(self, alg, signing_input, signature),
            "oct" => verify_hmac(self, alg, signing_input, signature),
            other => Err(SsoError::with_detail(MessageKey::UnknownSigningKey, other)),
        }
    }

    fn matched(&self, alg: Alg) -> bool {
        self.alg
            .as_deref()
            .is_none_or(|declared| declared == alg.name())
    }
}

fn decode(segment: Option<&String>) -> Result<Vec<u8>, SsoError> {
    let value = segment.ok_or_else(|| SsoError::new(MessageKey::UnknownSigningKey))?;
    URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| SsoError::new(MessageKey::UnknownSigningKey))
}

fn verify_rsa(key: &Jwk, alg: Alg, signing_input: &[u8], signature: &[u8]) -> Result<(), SsoError> {
    if !key.matched(alg) {
        return Err(SsoError::with_detail(
            MessageKey::UnsupportedAlgorithm,
            format!("key is pinned to {}", key.alg.clone().unwrap_or_default()),
        ));
    }
    let n = BigUint::from_bytes_be(&decode(key.n.as_ref())?);
    let e = BigUint::from_bytes_be(&decode(key.e.as_ref())?);
    let public =
        RsaPublicKey::new(n, e).map_err(|_| SsoError::new(MessageKey::UnknownSigningKey))?;
    let invalid = || SsoError::new(MessageKey::SignatureInvalid);
    match alg {
        Alg::Rs256 | Alg::Rs384 | Alg::Rs512 => {
            let hashed = digest(alg, signing_input);
            let scheme = match alg {
                Alg::Rs384 => Pkcs1v15Sign::new::<Sha384>(),
                Alg::Rs512 => Pkcs1v15Sign::new::<Sha512>(),
                _ => Pkcs1v15Sign::new::<Sha256>(),
            };
            public
                .verify(scheme, &hashed, signature)
                .map_err(|_| invalid())
        }
        Alg::Ps256 | Alg::Ps384 | Alg::Ps512 => {
            let hashed = digest(alg, signing_input);
            let scheme = match alg {
                Alg::Ps384 => Pss::new::<Sha384>(),
                Alg::Ps512 => Pss::new::<Sha512>(),
                _ => Pss::new::<Sha256>(),
            };
            public
                .verify(scheme, &hashed, signature)
                .map_err(|_| invalid())
        }
        _ => Err(SsoError::with_detail(
            MessageKey::UnsupportedAlgorithm,
            alg.name(),
        )),
    }
}

fn verify_ec(key: &Jwk, alg: Alg, signing_input: &[u8], signature: &[u8]) -> Result<(), SsoError> {
    if alg != Alg::Es256 {
        return Err(SsoError::with_detail(
            MessageKey::UnsupportedAlgorithm,
            alg.name(),
        ));
    }
    if key.crv.as_deref() != Some("P-256") || !key.matched(alg) {
        return Err(SsoError::with_detail(
            MessageKey::UnknownSigningKey,
            key.crv.clone().unwrap_or_default(),
        ));
    }
    let x = decode(key.x.as_ref())?;
    let y = decode(key.y.as_ref())?;
    let point =
        EncodedPoint::from_affine_coordinates(x.as_slice().into(), y.as_slice().into(), false);
    let verifying = EcKey::from_encoded_point(&point)
        .map_err(|_| SsoError::new(MessageKey::UnknownSigningKey))?;
    let signature = EcSignature::from_slice(signature)
        .map_err(|_| SsoError::new(MessageKey::SignatureInvalid))?;
    use p256::ecdsa::signature::Verifier;
    verifying
        .verify(signing_input, &signature)
        .map_err(|_| SsoError::new(MessageKey::SignatureInvalid))
}

fn verify_hmac(
    key: &Jwk,
    alg: Alg,
    signing_input: &[u8],
    signature: &[u8],
) -> Result<(), SsoError> {
    let secret = decode(key.k.as_ref())?;
    let invalid = || SsoError::new(MessageKey::SignatureInvalid);
    let computed = match alg {
        Alg::Hs256 => {
            let mut mac = Hmac::<Sha256>::new_from_slice(&secret).map_err(|_| invalid())?;
            mac.update(signing_input);
            mac.finalize().into_bytes().to_vec()
        }
        Alg::Hs384 => {
            let mut mac = Hmac::<Sha384>::new_from_slice(&secret).map_err(|_| invalid())?;
            mac.update(signing_input);
            mac.finalize().into_bytes().to_vec()
        }
        Alg::Hs512 => {
            let mut mac = Hmac::<Sha512>::new_from_slice(&secret).map_err(|_| invalid())?;
            mac.update(signing_input);
            mac.finalize().into_bytes().to_vec()
        }
        _ => {
            return Err(SsoError::with_detail(
                MessageKey::UnsupportedAlgorithm,
                alg.name(),
            ))
        }
    };
    if computed.ct_eq(signature).into() {
        Ok(())
    } else {
        Err(invalid())
    }
}

fn digest(alg: Alg, input: &[u8]) -> Vec<u8> {
    match alg {
        Alg::Rs384 | Alg::Ps384 => Sha384::digest(input).to_vec(),
        Alg::Rs512 | Alg::Ps512 => Sha512::digest(input).to_vec(),
        _ => Sha256::digest(input).to_vec(),
    }
}

/// A published key set. `refresh` is signalled by an unknown `kid`, so a rotated
/// provider key costs one refetch rather than a failed login.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct Jwks {
    #[serde(default)]
    pub keys: Vec<Jwk>,
}

impl Jwks {
    pub fn from_json(document: &str) -> Result<Jwks, SsoError> {
        serde_json::from_str(document).map_err(|_| SsoError::new(MessageKey::UnknownSigningKey))
    }

    pub fn select(&self, kid: Option<&str>, alg: Alg) -> Option<&Jwk> {
        let mut usable = self
            .keys
            .iter()
            .filter(|key| key.r#use.as_deref() != Some("enc"))
            .filter(|key| key.matched(alg));
        match kid {
            Some(kid) => usable.find(|key| key.kid.as_deref() == Some(kid)),
            // No kid: only safe when exactly one key could have signed.
            None => {
                let candidates: Vec<&Jwk> = usable.collect();
                if candidates.len() == 1 {
                    candidates.into_iter().next()
                } else {
                    None
                }
            }
        }
    }
}

/// Verify a token end to end against a key set: algorithm policy, key
/// selection, signature.
pub fn verify_token(token: &str, jwks: &Jwks, policy: &AlgPolicy) -> Result<ParsedJwt, SsoError> {
    let parsed = ParsedJwt::parse(token)?;
    let alg = parsed.alg()?;
    if !policy.allows(alg) {
        return Err(SsoError::with_detail(
            MessageKey::UnsupportedAlgorithm,
            alg.name(),
        ));
    }
    let key = jwks.select(parsed.kid(), alg).ok_or_else(|| {
        SsoError::with_detail(
            MessageKey::UnknownSigningKey,
            parsed.kid().unwrap_or("none"),
        )
    })?;
    parsed.verify(alg, key)?;
    Ok(parsed)
}
