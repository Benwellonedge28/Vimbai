//! Sessions: an absolute lifetime, an idle window, optional use-rotation,
//! per-user caps, address binding when asked, and an audit trail that records
//! why a session ended rather than just that it did.

use indexmap::IndexMap;

use crate::clock::to_iso;
use crate::crypto::sha256_hex;
use crate::error::SsoError;
use crate::i18n::{Locale, MessageKey};

const MICROS_PER_MINUTE: i64 = 60 * 1_000_000;

#[derive(Debug, Clone, PartialEq)]
pub struct SessionPolicy {
    /// How long a session may live in total, regardless of activity.
    pub absolute_ttl_micros: i64,
    /// How long it may sit unused.
    pub idle_ttl_micros: i64,
    /// Hand back a fresh token on every validated request, retiring the old one.
    pub rotate_on_use: bool,
    /// 0 means unlimited. Above the cap, the oldest session is ended.
    pub max_sessions_per_user: usize,
    /// Bind a session to the address that opened it. Off by default because
    /// mobile networks move, on for administrative roles.
    pub bind_to_address: bool,
}

impl Default for SessionPolicy {
    fn default() -> Self {
        SessionPolicy {
            absolute_ttl_micros: 8 * 60 * MICROS_PER_MINUTE,
            idle_ttl_micros: 30 * MICROS_PER_MINUTE,
            rotate_on_use: true,
            max_sessions_per_user: 10,
            bind_to_address: false,
        }
    }
}

impl SessionPolicy {
    pub fn strict() -> Self {
        SessionPolicy {
            absolute_ttl_micros: 60 * MICROS_PER_MINUTE,
            idle_ttl_micros: 10 * MICROS_PER_MINUTE,
            rotate_on_use: true,
            max_sessions_per_user: 3,
            bind_to_address: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Session {
    pub token: String,
    pub user_id: String,
    pub organization_id: String,
    pub idp_alias: String,
    pub roles: Vec<String>,
    pub locale: Locale,
    pub timezone: Option<String>,
    pub region: String,
    pub created_at_micros: i64,
    pub last_seen_at_micros: i64,
    pub absolute_expiry_micros: i64,
    pub idle_expiry_micros: i64,
    pub address: Option<String>,
    pub rotated_from: Option<String>,
}

impl Session {
    pub fn expires_at(&self) -> String {
        to_iso(self.absolute_expiry_micros)
    }

    /// The body `/session/{token}` returns. No roles: a token holder already has
    /// them, and a leaked response should not restate them.
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "valid": true,
            "user_id": self.user_id,
            "org_id": self.organization_id,
            "org": self.organization_id,
            "provider": self.idp_alias,
            "region": self.region,
            "locale": self.locale.to_string(),
            "timezone": self.timezone,
            "roles": self.roles,
            "expires_at": self.expires_at(),
            "idle_until": to_iso(self.idle_expiry_micros),
        })
    }
}

/// Why something happened, for the audit trail and for incident review.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditKind {
    Authenticated,
    SessionValidated,
    SessionRotated,
    SessionRevoked,
    SessionRevokedAll,
    SessionExpired,
    SessionIdle,
    SessionAddressMismatch,
    SessionEvicted,
    AuthenticationRejected,
}

impl AuditKind {
    pub fn code(self) -> &'static str {
        match self {
            AuditKind::Authenticated => "authenticated",
            AuditKind::SessionValidated => "session_validated",
            AuditKind::SessionRotated => "session_rotated",
            AuditKind::SessionRevoked => "session_revoked",
            AuditKind::SessionRevokedAll => "sessions_revoked_all",
            AuditKind::SessionExpired => "session_expired",
            AuditKind::SessionIdle => "session_idle",
            AuditKind::SessionAddressMismatch => "session_address_mismatch",
            AuditKind::SessionEvicted => "session_evicted",
            AuditKind::AuthenticationRejected => "authentication_rejected",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AuditEvent {
    pub at_micros: i64,
    pub kind: AuditKind,
    pub organization_id: String,
    pub idp_alias: String,
    pub user_id: Option<String>,
    pub code: Option<String>,
}

impl AuditEvent {
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "at": to_iso(self.at_micros),
            "event": self.kind.code(),
            "organization_id": self.organization_id,
            "provider": self.idp_alias,
            "user_id": self.user_id,
            "code": self.code,
        })
    }
}

/// The caller's sessions. Nothing is global, so one Book cannot see another's.
#[derive(Debug, Clone)]
pub struct SessionStore {
    sessions: IndexMap<String, Session>,
    audit: Vec<AuditEvent>,
    /// Bounds the audit trail so a long-running process cannot grow without end.
    audit_limit: usize,
}

impl Default for SessionStore {
    fn default() -> Self {
        SessionStore::new()
    }
}

impl SessionStore {
    pub fn new() -> Self {
        SessionStore {
            sessions: IndexMap::new(),
            audit: Vec::new(),
            audit_limit: 10_000,
        }
    }

    pub fn active(&self) -> usize {
        self.sessions.len()
    }

    pub fn tokens(&self) -> Vec<String> {
        self.sessions.keys().cloned().collect()
    }

    pub fn session(&self, token: &str) -> Option<&Session> {
        self.sessions.get(token)
    }

    pub fn audit(&self) -> &[AuditEvent] {
        &self.audit
    }

    pub fn record(&mut self, event: AuditEvent) {
        if self.audit.len() >= self.audit_limit {
            self.audit.remove(0);
        }
        self.audit.push(event);
    }

    /// Open a session. `entropy` must be unpredictable: it is mixed with the
    /// identity to derive the token, and the caller owns the randomness so tests
    /// can reproduce a token exactly.
    #[allow(clippy::too_many_arguments)]
    pub fn open(
        &mut self,
        policy: &SessionPolicy,
        user_id: &str,
        organization_id: &str,
        idp_alias: &str,
        roles: Vec<String>,
        locale: Locale,
        timezone: Option<String>,
        region: &str,
        now_micros: i64,
        entropy: &str,
        address: Option<String>,
    ) -> Session {
        if policy.max_sessions_per_user > 0 {
            self.evict_oldest_beyond(policy.max_sessions_per_user, user_id, now_micros);
        }
        // A token must be unique even when a caller reuses entropy: an existing
        // session must never be silently replaced by a new login.
        let mut token = sha256_hex(&format!(
            "{user_id}\u{1f}{organization_id}\u{1f}{now_micros}\u{1f}{entropy}"
        ));
        let mut attempt = 0;
        while self.sessions.contains_key(&token) {
            attempt += 1;
            token = sha256_hex(&format!(
                "{user_id}\u{1f}{organization_id}\u{1f}{now_micros}\u{1f}{entropy}\u{1f}{attempt}"
            ));
        }
        let session = Session {
            token: token.clone(),
            user_id: user_id.to_string(),
            organization_id: organization_id.to_string(),
            idp_alias: idp_alias.to_string(),
            roles,
            locale,
            timezone,
            region: region.to_string(),
            created_at_micros: now_micros,
            last_seen_at_micros: now_micros,
            absolute_expiry_micros: now_micros + policy.absolute_ttl_micros,
            idle_expiry_micros: now_micros + policy.idle_ttl_micros,
            address,
            rotated_from: None,
        };
        self.sessions.insert(token, session.clone());
        session
    }

    /// Check a session and, when the policy asks for it, hand back a rotated
    /// token. Returns the session to use and the token that was retired.
    pub fn validate(
        &mut self,
        token: &str,
        now_micros: i64,
        policy: &SessionPolicy,
        address: Option<&str>,
        entropy: &str,
    ) -> Result<(Session, Option<String>), SsoError> {
        let existing = self
            .sessions
            .get(token)
            .cloned()
            .ok_or_else(|| SsoError::new(MessageKey::InvalidSession))?;

        if existing.absolute_expiry_micros <= now_micros {
            self.sessions.shift_remove(token);
            self.record(AuditEvent {
                at_micros: now_micros,
                kind: AuditKind::SessionExpired,
                organization_id: existing.organization_id.clone(),
                idp_alias: existing.idp_alias.clone(),
                user_id: Some(existing.user_id.clone()),
                code: Some(MessageKey::SessionExpired.code().to_string()),
            });
            return Err(SsoError::new(MessageKey::SessionExpired));
        }
        if existing.idle_expiry_micros <= now_micros {
            self.sessions.shift_remove(token);
            self.record(AuditEvent {
                at_micros: now_micros,
                kind: AuditKind::SessionIdle,
                organization_id: existing.organization_id.clone(),
                idp_alias: existing.idp_alias.clone(),
                user_id: Some(existing.user_id.clone()),
                code: Some(MessageKey::SessionIdle.code().to_string()),
            });
            return Err(SsoError::new(MessageKey::SessionIdle));
        }
        if policy.bind_to_address {
            if let (Some(bound), Some(presented)) = (existing.address.as_deref(), address) {
                if bound != presented {
                    self.record(AuditEvent {
                        at_micros: now_micros,
                        kind: AuditKind::SessionAddressMismatch,
                        organization_id: existing.organization_id.clone(),
                        idp_alias: existing.idp_alias.clone(),
                        user_id: Some(existing.user_id.clone()),
                        code: Some(MessageKey::SessionIpMismatch.code().to_string()),
                    });
                    return Err(SsoError::new(MessageKey::SessionIpMismatch));
                }
            }
        }

        if !policy.rotate_on_use {
            if let Some(slot) = self.sessions.get_mut(token) {
                slot.last_seen_at_micros = now_micros;
                slot.idle_expiry_micros =
                    (now_micros + policy.idle_ttl_micros).min(slot.absolute_expiry_micros);
                let updated = slot.clone();
                self.record(AuditEvent {
                    at_micros: now_micros,
                    kind: AuditKind::SessionValidated,
                    organization_id: updated.organization_id.clone(),
                    idp_alias: updated.idp_alias.clone(),
                    user_id: Some(updated.user_id.clone()),
                    code: None,
                });
                return Ok((updated, None));
            }
            return Err(SsoError::new(MessageKey::InvalidSession));
        }

        let rotated = Session {
            token: sha256_hex(&format!(
                "{}\u{1f}{now_micros}\u{1f}{entropy}",
                existing.user_id
            )),
            created_at_micros: existing.created_at_micros,
            last_seen_at_micros: now_micros,
            absolute_expiry_micros: existing.absolute_expiry_micros,
            idle_expiry_micros: (now_micros + policy.idle_ttl_micros)
                .min(existing.absolute_expiry_micros),
            rotated_from: Some(existing.token.clone()),
            ..existing.clone()
        };
        self.sessions.shift_remove(token);
        self.sessions.insert(rotated.token.clone(), rotated.clone());
        self.record(AuditEvent {
            at_micros: now_micros,
            kind: AuditKind::SessionRotated,
            organization_id: rotated.organization_id.clone(),
            idp_alias: rotated.idp_alias.clone(),
            user_id: Some(rotated.user_id.clone()),
            code: None,
        });
        Ok((rotated, Some(existing.token)))
    }

    pub fn revoke(&mut self, token: &str, now_micros: i64) -> bool {
        match self.sessions.shift_remove(token) {
            Some(session) => {
                self.record(AuditEvent {
                    at_micros: now_micros,
                    kind: AuditKind::SessionRevoked,
                    organization_id: session.organization_id,
                    idp_alias: session.idp_alias,
                    user_id: Some(session.user_id),
                    code: None,
                });
                true
            }
            None => false,
        }
    }

    pub fn revoke_all_for_user(&mut self, user_id: &str, now_micros: i64) -> usize {
        let victims: Vec<String> = self
            .sessions
            .iter()
            .filter(|(_, session)| session.user_id == user_id)
            .map(|(token, _)| token.clone())
            .collect();
        for token in &victims {
            if let Some(session) = self.sessions.shift_remove(token) {
                self.record(AuditEvent {
                    at_micros: now_micros,
                    kind: AuditKind::SessionRevokedAll,
                    organization_id: session.organization_id,
                    idp_alias: session.idp_alias,
                    user_id: Some(session.user_id),
                    code: None,
                });
            }
        }
        victims.len()
    }

    pub fn prune(&mut self, now_micros: i64) -> usize {
        let expired: Vec<String> = self
            .sessions
            .iter()
            .filter(|(_, session)| {
                session.absolute_expiry_micros <= now_micros
                    || session.idle_expiry_micros <= now_micros
            })
            .map(|(token, _)| token.clone())
            .collect();
        for token in &expired {
            self.sessions.shift_remove(token);
        }
        expired.len()
    }

    fn evict_oldest_beyond(&mut self, limit: usize, user_id: &str, now_micros: i64) {
        let mut held: Vec<(String, i64)> = self
            .sessions
            .iter()
            .filter(|(_, session)| session.user_id == user_id)
            .map(|(token, session)| (token.clone(), session.created_at_micros))
            .collect();
        if held.len() < limit {
            return;
        }
        held.sort_by_key(|(_, created)| *created);
        let excess = held.len() - limit + 1;
        for (token, _) in held.into_iter().take(excess) {
            if let Some(session) = self.sessions.shift_remove(&token) {
                self.record(AuditEvent {
                    at_micros: now_micros,
                    kind: AuditKind::SessionEvicted,
                    organization_id: session.organization_id,
                    idp_alias: session.idp_alias,
                    user_id: Some(session.user_id),
                    code: Some(MessageKey::ConcurrentSessionEvicted.code().to_string()),
                });
            }
        }
    }
}
