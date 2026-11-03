//! SAML 2.0 assertions: parse the parts that decide access, then verify the
//! signature over the exact bytes received.
//!
//! The service trusted any string longer than twenty characters. This reads the
//! assertion properly and refuses one whose signature, window, or audience does
//! not hold. An enveloped signature inside a larger document needs XML
//! canonicalisation, which is not implemented: supply the assertion element.

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use quick_xml::events::Event;
use quick_xml::Reader;
use serde_json::{json, Value};

use crate::clock::parse_iso;
use crate::error::SsoError;
use crate::i18n::MessageKey;
use crate::jose::{Alg, Jwk};

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SamlAssertion {
    pub id: Option<String>,
    pub version: Option<String>,
    pub issue_instant: Option<String>,
    pub issuer: Option<String>,
    pub name_id: Option<String>,
    pub name_id_format: Option<String>,
    pub not_before: Option<String>,
    pub not_on_or_after: Option<String>,
    pub audiences: Vec<String>,
    pub attributes: Vec<(String, Vec<String>)>,
}

impl SamlAssertion {
    pub fn attribute(&self, name: &str) -> Vec<String> {
        self.attributes
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, values)| values.clone())
            .unwrap_or_default()
    }

    pub fn email(&self) -> Option<String> {
        self.attribute("email")
            .into_iter()
            .next()
            .or_else(|| self.name_id.clone())
    }

    pub fn to_json(&self) -> Value {
        json!({
            "id": self.id,
            "issuer": self.issuer,
            "name_id": self.name_id,
            "audiences": self.audiences,
            "attributes": self.attributes,
        })
    }
}

/// Parse an assertion. Element prefixes are ignored, so both `saml:Assertion`
/// and a default-namespaced document read the same.
pub fn parse_assertion(xml: &str) -> Result<SamlAssertion, SsoError> {
    if !xml.contains("Assertion") {
        return Err(SsoError::with_detail(
            MessageKey::InvalidSamlAssertion,
            "no assertion element",
        ));
    }
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut assertion = SamlAssertion::default();
    let mut stack: Vec<String> = Vec::new();
    let mut current_attribute: Option<String> = None;
    let mut text_target: Option<String> = None;
    let mut buffer = String::new();

    loop {
        match reader.read_event() {
            Ok(Event::Eof) => break,
            Ok(Event::Start(element)) => {
                let name = String::from_utf8_lossy(element.local_name().as_ref()).to_string();
                if name == "Assertion" {
                    for attribute in element.attributes().flatten() {
                        let key = String::from_utf8_lossy(attribute.key.local_name().as_ref())
                            .to_string();
                        let value = attribute
                            .unescape_value()
                            .map(|v| v.to_string())
                            .unwrap_or_default();
                        match key.as_str() {
                            "ID" => assertion.id = Some(value),
                            "Version" => assertion.version = Some(value),
                            "IssueInstant" => assertion.issue_instant = Some(value),
                            _ => {}
                        }
                    }
                }
                if name == "Conditions" {
                    for attribute in element.attributes().flatten() {
                        let key = String::from_utf8_lossy(attribute.key.local_name().as_ref())
                            .to_string();
                        let value = attribute
                            .unescape_value()
                            .map(|v| v.to_string())
                            .unwrap_or_default();
                        match key.as_str() {
                            "NotBefore" => assertion.not_before = Some(value),
                            "NotOnOrAfter" => assertion.not_on_or_after = Some(value),
                            _ => {}
                        }
                    }
                }
                if name == "NameID" {
                    for attribute in element.attributes().flatten() {
                        if attribute.key.local_name().as_ref() == b"Format" {
                            assertion.name_id_format =
                                attribute.unescape_value().map(|v| v.to_string()).ok();
                        }
                    }
                    text_target = Some("NameID".to_string());
                }
                if name == "Attribute" {
                    for attribute in element.attributes().flatten() {
                        if attribute.key.local_name().as_ref() == b"Name" {
                            current_attribute =
                                attribute.unescape_value().map(|v| v.to_string()).ok();
                        }
                    }
                }
                if name == "AttributeValue" && current_attribute.is_some() {
                    text_target = Some("AttributeValue".to_string());
                }
                if name == "Issuer" {
                    text_target = Some("Issuer".to_string());
                }
                buffer.clear();
                stack.push(name);
            }
            Ok(Event::Empty(element)) => {
                let name = String::from_utf8_lossy(element.local_name().as_ref()).to_string();
                if name == "Conditions" {
                    for attribute in element.attributes().flatten() {
                        let key = String::from_utf8_lossy(attribute.key.local_name().as_ref())
                            .to_string();
                        let value = attribute
                            .unescape_value()
                            .map(|v| v.to_string())
                            .unwrap_or_default();
                        match key.as_str() {
                            "NotBefore" => assertion.not_before = Some(value),
                            "NotOnOrAfter" => assertion.not_on_or_after = Some(value),
                            _ => {}
                        }
                    }
                }
                buffer.clear();
            }
            Ok(Event::Text(text)) => {
                if let Ok(value) = text.unescape() {
                    buffer.push_str(&value);
                }
            }
            Ok(Event::End(element)) => {
                let name = String::from_utf8_lossy(element.local_name().as_ref()).to_string();
                match (name.as_str(), text_target.take()) {
                    ("NameID", Some(_)) if assertion.name_id.is_none() => {
                        assertion.name_id = Some(buffer.clone());
                    }
                    ("Issuer", Some(_)) if assertion.issuer.is_none() => {
                        assertion.issuer = Some(buffer.clone());
                    }
                    ("AttributeValue", Some(_)) => {
                        if let Some(attribute) = &current_attribute {
                            let entry = assertion
                                .attributes
                                .iter_mut()
                                .find(|(key, _)| key == attribute);
                            match entry {
                                Some((_, values)) => values.push(buffer.clone()),
                                None => assertion
                                    .attributes
                                    .push((attribute.clone(), vec![buffer.clone()])),
                            }
                        }
                    }
                    ("Audience", _) => assertion.audiences.push(buffer.clone()),
                    ("Attribute", _) => current_attribute = None,
                    _ => {}
                }
                buffer.clear();
                stack.pop();
            }
            Err(error) => {
                return Err(SsoError::with_detail(
                    MessageKey::InvalidSamlAssertion,
                    error.to_string(),
                ));
            }
            _ => {}
        }
    }

    if assertion.issuer.is_none() {
        return Err(SsoError::with_detail(
            MessageKey::InvalidSamlAssertion,
            "issuer missing",
        ));
    }
    Ok(assertion)
}

/// Check the assertion against the organisation's rules and clock.
pub struct AssertionPolicy<'a> {
    pub issuer: String,
    /// Any of these satisfies the audience restriction.
    pub audiences: Vec<String>,
    pub leeway_seconds: i64,
    pub allowed_email_domains: &'a [String],
    pub max_age_seconds: Option<i64>,
}

impl<'a> AssertionPolicy<'a> {
    pub fn check(&self, assertion: &SamlAssertion, now_micros: i64) -> Result<(), SsoError> {
        let now_seconds = now_micros as f64 / 1_000_000.0;
        let leeway = self.leeway_seconds as f64;

        let issuer = assertion
            .issuer
            .as_deref()
            .ok_or_else(|| SsoError::with_detail(MessageKey::MissingClaim, "Issuer"))?;
        if issuer.trim_end_matches('/') != self.issuer.trim_end_matches('/') {
            return Err(SsoError::with_detail(MessageKey::IssuerMismatch, issuer));
        }

        if !self.audiences.is_empty()
            && !assertion
                .audiences
                .iter()
                .any(|aud| self.audiences.contains(aud))
        {
            return Err(SsoError::with_detail(
                MessageKey::AudienceRestrictionFailed,
                assertion.audiences.join(","),
            ));
        }

        if let Some(not_before) = assertion.not_before.as_deref() {
            let stamp = parse_iso(not_before).ok_or_else(|| {
                SsoError::with_detail(MessageKey::AssertionConditionsFailed, "NotBefore")
            })?;
            if (stamp as f64 / 1_000_000.0) - leeway > now_seconds {
                return Err(SsoError::with_detail(
                    MessageKey::AssertionConditionsFailed,
                    "NotBefore in the future",
                ));
            }
        }
        if let Some(not_on_or_after) = assertion.not_on_or_after.as_deref() {
            let stamp = parse_iso(not_on_or_after).ok_or_else(|| {
                SsoError::with_detail(MessageKey::AssertionConditionsFailed, "NotOnOrAfter")
            })?;
            if (stamp as f64 / 1_000_000.0) + leeway < now_seconds {
                return Err(SsoError::new(MessageKey::AssertionConditionsFailed));
            }
        }
        if let Some(max_age) = self.max_age_seconds {
            if let Some(issued) = assertion.issue_instant.as_deref().and_then(parse_iso) {
                if (now_micros - issued) > max_age * 1_000_000 {
                    return Err(SsoError::with_detail(
                        MessageKey::AssertionConditionsFailed,
                        "IssueInstant too old",
                    ));
                }
            }
        }

        if !self.allowed_email_domains.is_empty() {
            let permitted = assertion
                .email()
                .and_then(|address| {
                    address
                        .rsplit_once('@')
                        .map(|(_, domain)| domain.to_string())
                })
                .map(|domain| {
                    self.allowed_email_domains
                        .iter()
                        .any(|allowed| allowed.eq_ignore_ascii_case(&domain))
                })
                .unwrap_or(false);
            if !permitted {
                return Err(SsoError::with_detail(
                    MessageKey::EmailDomainNotAllowed,
                    assertion.email().unwrap_or_default(),
                ));
            }
        }
        Ok(())
    }
}

/// Verify the IdP's signature over the assertion bytes with any of the
/// registered keys. RSA-SHA256 is what SAML deployments sign with.
pub fn verify_signature(
    assertion_bytes: &[u8],
    signature_b64: &str,
    keys: &[Jwk],
) -> Result<(), SsoError> {
    let signature = BASE64
        .decode(signature_b64.trim())
        .map_err(|_| SsoError::new(MessageKey::InvalidSamlAssertion))?;
    if keys.is_empty() {
        return Err(SsoError::with_detail(
            MessageKey::UnknownSigningKey,
            "no SAML signing key registered",
        ));
    }
    let mut last = SsoError::new(MessageKey::SignatureInvalid);
    for key in keys {
        match key.verify(Alg::Rs256, assertion_bytes, &signature) {
            Ok(()) => return Ok(()),
            Err(error) => last = error,
        }
    }
    Err(last)
}
