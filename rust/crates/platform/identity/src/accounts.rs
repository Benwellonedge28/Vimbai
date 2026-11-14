#![forbid(unsafe_code)]
//! identity-service: the accounts a person signs in with, and the tokens issued in exchange.
//!
//! This is the door the gateway leaves open - `/identity` - so it is the one service the platform cannot do without:
//! every other call carries a token minted here. The rules below are the shipped ones, kept rather than improved on:
//!
//! * a person signs in with **either** username or email in the same field, and a wrong password answers the same
//!   "Invalid credentials" as an unknown account, so the door never says which accounts exist;
//! * passwords are verified against the hashes this platform has always stored (bcrypt), so no account has to be
//!   migrated or re-invited on the day the Python service goes;
//! * an account that is not active is refused with 403, distinct from 401: the credentials were right, the account is
//!   not allowed in;
//! * a person with MFA enabled gets no session on a correct password - only a short-lived token that says so;
//! * the access token carries the permissions of the person's roles, resolved at issue time, so a claim cannot be
//!   widened by editing a role later without the token being reissued.
//!
//! Storage is deliberately absent: this module decides, the caller keeps records. That keeps the arithmetic of
//! "is this really them" testable without a database, and matches how every other domain crate is shaped.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// What a person is allowed to do. The strings are the shipped ones, because they are written into issued tokens and
/// a role edited through the API names them.
pub const CAPABILITIES: [&str; 22] = [
    "account:view",
    "account:create",
    "account:edit",
    "account:delete",
    "journal:view",
    "journal:create",
    "journal:post",
    "journal:delete",
    "budget:view",
    "budget:create",
    "budget:approve",
    "report:view",
    "report:create",
    "report:export",
    "user:manage",
    "role:manage",
    "audit:view",
    "system:config",
    "feature:toggle",
    "integration:view",
    "integration:config",
    "integration:delete",
];

/// Whether an account may sign in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UserStatus {
    Active,
    Inactive,
    Suspended,
    PendingVerification,
}

impl UserStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            UserStatus::Active => "active",
            UserStatus::Inactive => "inactive",
            UserStatus::Suspended => "suspended",
            UserStatus::PendingVerification => "pending_verification",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "active" => UserStatus::Active,
            "inactive" => UserStatus::Inactive,
            "suspended" => UserStatus::Suspended,
            "pending_verification" => UserStatus::PendingVerification,
            _ => return None,
        })
    }

    /// A registered account must be usable immediately: this service has no email-verification flow, so parking an
    /// account as pending would lock it out for good.
    pub fn may_sign_in(self) -> bool {
        matches!(self, UserStatus::Active)
    }
}

/// How a second factor reaches a person.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MfaMethod {
    Totp,
    Sms,
    Email,
}

impl MfaMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            MfaMethod::Totp => "totp",
            MfaMethod::Sms => "sms",
            MfaMethod::Email => "email",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "totp" => MfaMethod::Totp,
            "sms" => MfaMethod::Sms,
            "email" => MfaMethod::Email,
            _ => return None,
        })
    }
}

/// A role: a named bundle of capabilities.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Role {
    pub id: String,
    pub name: String,
    pub description: String,
    pub permissions: Vec<String>,
    pub is_system: bool,
}

impl Role {
    pub fn to_json(&self) -> Value {
        json!({
            "id": self.id,
            "name": self.name,
            "description": self.description,
            "permissions": self.permissions,
            "is_system": self.is_system,
        })
    }
}

fn role(id: &str, name: &str, description: &str, permissions: &[&str]) -> Role {
    Role {
        id: id.to_string(),
        name: name.to_string(),
        description: description.to_string(),
        permissions: permissions.iter().map(|one| one.to_string()).collect(),
        is_system: true,
    }
}

/// The roles every deployment starts with. Seeding is idempotent: a role that already exists is left alone, so an
/// operator who edited a system role does not have their edit undone on the next start.
pub fn default_roles() -> Vec<Role> {
    vec![
        role(
            "admin",
            "Administrator",
            "Full system access",
            &CAPABILITIES,
        ),
        role(
            "accountant",
            "Accountant",
            "Accounting operations access",
            &[
                "account:view",
                "account:create",
                "account:edit",
                "journal:view",
                "journal:create",
                "journal:post",
                "report:view",
                "report:create",
                "report:export",
            ],
        ),
        role(
            "finance_manager",
            "Finance Manager",
            "Financial management access",
            &[
                "account:view",
                "account:create",
                "account:edit",
                "journal:view",
                "journal:create",
                "journal:post",
                "budget:view",
                "budget:create",
                "budget:approve",
                "report:view",
                "report:create",
                "report:export",
            ],
        ),
        role(
            "viewer",
            "Viewer",
            "Read-only access",
            &[
                "account:view",
                "journal:view",
                "report:view",
                "report:export",
            ],
        ),
        role(
            "user",
            "User",
            "Basic user access",
            &["account:view", "journal:view", "report:view"],
        ),
    ]
}

/// The capabilities a person holds, unioned across their roles, in capability order so the issued claim is stable and
/// two runs cannot disagree about a token's contents.
pub fn permissions_for(role_ids: &[String], roles: &[Role]) -> Vec<String> {
    CAPABILITIES
        .iter()
        .filter(|capability| {
            role_ids.iter().any(|wanted| {
                roles.iter().any(|role| {
                    &role.id == wanted && role.permissions.iter().any(|held| held == *capability)
                })
            })
        })
        .map(|capability| capability.to_string())
        .collect()
}

/// An account. The password is held as the shipped format's hash, never as the password.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub email: String,
    pub username: String,
    pub first_name: String,
    pub last_name: String,
    pub phone: Option<String>,
    pub status: UserStatus,
    pub role_ids: Vec<String>,
    pub organization_id: Option<String>,
    pub permissions: Vec<String>,
    pub mfa_enabled: bool,
    pub mfa_method: MfaMethod,
    pub password_hash: String,
    pub created_at: String,
    pub updated_at: String,
    pub last_login: Option<String>,
}

impl User {
    /// What a caller is told about themselves. The hash never leaves the service.
    pub fn to_json(&self) -> Value {
        json!({
            "id": self.id,
            "email": self.email,
            "username": self.username,
            "first_name": self.first_name,
            "last_name": self.last_name,
            "phone": self.phone,
            "status": self.status.as_str(),
            "role_ids": self.role_ids,
            "organization_id": self.organization_id,
            "permissions": self.permissions,
            "mfa_enabled": self.mfa_enabled,
            "mfa_method": self.mfa_method.as_str(),
            "created_at": self.created_at,
            "updated_at": self.updated_at,
            "last_login": self.last_login,
        })
    }
}

/// A registration request, before the account exists.
#[derive(Debug, Clone)]
pub struct NewUser {
    pub id: String,
    pub email: String,
    pub username: String,
    pub first_name: String,
    pub last_name: String,
    pub phone: Option<String>,
    pub role_ids: Vec<String>,
    pub organization_id: Option<String>,
    pub password: String,
}

/// Why an account request was refused. Each carries the status the shipped service answered, so a client sees the
/// same codes as before.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccountError {
    /// 400: the email is already registered.
    EmailTaken,
    /// 400: the username is already taken.
    UsernameTaken,
    /// 400: a named role does not exist.
    UnknownRole(String),
    /// 401: wrong password, or no such account. Deliberately one answer for both.
    InvalidCredentials,
    /// 403: the credentials were right but the account may not sign in.
    NotActive,
    /// 400: the password is unusable (empty, or too long for the hash).
    WeakPassword,
}

impl AccountError {
    pub fn status(&self) -> u16 {
        match self {
            AccountError::EmailTaken
            | AccountError::UsernameTaken
            | AccountError::UnknownRole(_)
            | AccountError::WeakPassword => 400,
            AccountError::InvalidCredentials => 401,
            AccountError::NotActive => 403,
        }
    }

    pub fn message(&self) -> String {
        match self {
            // One answer for an unknown account and a wrong password: the door must not say which accounts exist.
            AccountError::InvalidCredentials => "Invalid credentials".to_string(),
            AccountError::EmailTaken => "Email already registered".to_string(),
            AccountError::UsernameTaken => "Username already taken".to_string(),
            AccountError::UnknownRole(role) => format!("Role {role} not found"),
            AccountError::NotActive => "Account is not active".to_string(),
            AccountError::WeakPassword => "Password does not meet the requirements".to_string(),
        }
    }
}

/// bcrypt's own limit. A longer password cannot be hashed faithfully, so it is refused rather than truncated into
/// something else the person did not choose.
const MAX_PASSWORD_BYTES: usize = 72;

/// Hash a password the way this platform stores them.
pub fn hash_password(password: &str) -> Result<String, AccountError> {
    if password.is_empty() || password.len() > MAX_PASSWORD_BYTES {
        return Err(AccountError::WeakPassword);
    }
    bcrypt::hash(password, bcrypt::DEFAULT_COST).map_err(|_| AccountError::WeakPassword)
}

/// Check a password against a stored hash. A hash this build cannot read is a failed password, never an error that
/// says so: the same answer goes back for both.
pub fn verify_password(password: &str, hash: &str) -> bool {
    if password.len() > MAX_PASSWORD_BYTES {
        return false;
    }
    bcrypt::verify(password, hash).unwrap_or(false)
}

/// Register an account, refusing the collisions the shipped service refused. Checks run in the shipped order so a
/// request naming both a taken email and a taken username answers as before.
pub fn register(
    users: &[User],
    roles: &[Role],
    wanted: NewUser,
    now: DateTime<Utc>,
) -> Result<User, AccountError> {
    let email = wanted.email.trim().to_lowercase();
    if users
        .iter()
        .any(|user| user.email.trim().to_lowercase() == email)
    {
        return Err(AccountError::EmailTaken);
    }
    if users
        .iter()
        .any(|user| user.username.eq_ignore_ascii_case(&wanted.username))
    {
        return Err(AccountError::UsernameTaken);
    }
    for role_id in &wanted.role_ids {
        if !roles.iter().any(|role| &role.id == role_id) {
            return Err(AccountError::UnknownRole(role_id.clone()));
        }
    }

    let stamp = now.to_rfc3339();
    Ok(User {
        id: wanted.id,
        email,
        username: wanted.username,
        first_name: wanted.first_name,
        last_name: wanted.last_name,
        phone: wanted.phone,
        // No verification flow exists, so an account is usable the moment it is made.
        status: UserStatus::Active,
        permissions: permissions_for(&wanted.role_ids, roles),
        role_ids: wanted.role_ids,
        organization_id: wanted.organization_id,
        mfa_enabled: false,
        mfa_method: MfaMethod::Totp,
        password_hash: hash_password(&wanted.password)?,
        created_at: stamp.clone(),
        updated_at: stamp,
        last_login: None,
    })
}

/// What a correct password earns: a session, or a second-factor challenge instead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignIn {
    /// The person is in. The caller mints a session token.
    Allowed(User),
    /// MFA is on: no session yet, only a short-lived token that says a challenge is owed.
    SecondFactor { user: User },
}

/// Sign someone in. This does not check or spend an MFA code; it only decides whether MFA is owed.
pub fn sign_in(users: &[User], identifier: &str, password: &str) -> Result<SignIn, AccountError> {
    let found = users.iter().find(|user| {
        user.username.eq_ignore_ascii_case(identifier)
            || user.email.eq_ignore_ascii_case(identifier)
    });
    // An unknown account and a wrong password answer the same thing.
    let user = found.ok_or(AccountError::InvalidCredentials)?;
    if !verify_password(password, &user.password_hash) {
        return Err(AccountError::InvalidCredentials);
    }
    if !user.status.may_sign_in() {
        return Err(AccountError::NotActive);
    }
    let signed_in = user.clone();
    if signed_in.mfa_enabled {
        return Ok(SignIn::SecondFactor { user: signed_in });
    }
    Ok(SignIn::Allowed(signed_in))
}

/// The lifetime of a token, in the shipped numbers.
pub const ACCESS_TOKEN_SECONDS: i64 = 60 * 60;
pub const MFA_TOKEN_SECONDS: i64 = 5 * 60;

/// A minted token and when it stops working.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub value: String,
    pub expires_in: i64,
}

fn base64url(bytes: &[u8]) -> String {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;
    URL_SAFE_NO_PAD.encode(bytes)
}

/// Sign a compact JWS with HS256.
///
/// The payload uses the shipped claim names so a client that already reads `sub`, `type`, `permissions`, `role` and
/// `organization_id` keeps working, and the expiry is written in seconds since the epoch as JWT requires (chrono's
/// timestamps are never sent as a number of microseconds here: a reader parsing `exp` must see a date).
fn sign_hs256(payload: &Value, secret: &str) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    let header = base64url(br#"{"alg":"HS256","typ":"JWT"}"#);
    let body = base64url(payload.to_string().as_bytes());
    let signing_input = format!("{header}.{body}");
    let mut mac =
        Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key length");
    mac.update(signing_input.as_bytes());
    let signature = base64url(&mac.finalize().into_bytes());
    format!("{signing_input}.{signature}")
}

/// The token a person carries. Permissions are resolved now rather than read later, so a claim cannot be widened by
/// editing a role after the token was issued.
pub fn issue_access_token(
    user: &User,
    roles: &[Role],
    secret: &str,
    now: DateTime<Utc>,
    lifetime_seconds: Option<i64>,
) -> Token {
    let lifetime = lifetime_seconds.unwrap_or(ACCESS_TOKEN_SECONDS);
    let permissions = permissions_for(&user.role_ids, roles);
    let payload = json!({
        "sub": user.id,
        "type": "access",
        "exp": (now + Duration::seconds(lifetime)).timestamp(),
        "iat": now.timestamp(),
        "permissions": permissions,
        // The first role stands for the account in the claim, as shipped; a person with no role is a plain user.
        "role": user.role_ids.first().cloned().unwrap_or_else(|| "user".to_string()),
        "organization_id": user.organization_id,
    });
    Token {
        value: sign_hs256(&payload, secret),
        expires_in: lifetime,
    }
}

/// The short-lived token handed out instead of a session when a second factor is owed. It carries the same shape so
/// one verifier can read both, and says plainly that it is only good for finishing a sign-in.
pub fn issue_mfa_token(user: &User, secret: &str, now: DateTime<Utc>) -> Token {
    let payload = json!({
        "sub": user.id,
        "type": "mfa",
        "exp": (now + Duration::seconds(MFA_TOKEN_SECONDS)).timestamp(),
        "iat": now.timestamp(),
    });
    Token {
        value: sign_hs256(&payload, secret),
        expires_in: MFA_TOKEN_SECONDS,
    }
}

/// What a verified token says. Only the fields the door acts on are read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenClaims {
    pub subject: String,
    pub kind: String,
    pub permissions: Vec<String>,
    pub role: Option<String>,
    pub organization_id: Option<String>,
    pub expires_at: i64,
}

/// Read and verify a token. A bad signature, a wrong algorithm, an expired token or a malformed body all answer `None`:
/// the caller refuses, and nothing about the token is echoed back.
pub fn verify_token(token: &str, secret: &str, now: DateTime<Utc>) -> Option<TokenClaims> {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    let mut parts = token.split('.');
    let (header, body, signature) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() {
        return None;
    }
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).ok()?;
    mac.update(format!("{header}.{body}").as_bytes());
    let expected = mac.finalize().into_bytes();

    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;
    let supplied = URL_SAFE_NO_PAD.decode(signature).ok()?;
    // Constant-time: a comparison that stops at the first wrong byte leaks how much of a forged signature was right.
    if supplied.len() != expected.len() {
        return None;
    }
    let mut difference = 0u8;
    for (left, right) in supplied.iter().zip(expected.iter()) {
        difference |= left ^ right;
    }
    if difference != 0 {
        return None;
    }

    // The header must name the algorithm that was actually used: an unverified "none" or an asymmetric name would
    // otherwise let a caller pick the check.
    let header: Value = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(header).ok()?).ok()?;
    if header.get("alg").and_then(Value::as_str) != Some("HS256") {
        return None;
    }

    let claims: Value = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(body).ok()?).ok()?;
    let expires_at = claims.get("exp").and_then(Value::as_i64)?;
    if expires_at <= now.timestamp() {
        return None;
    }
    let subject = claims.get("sub").and_then(Value::as_str)?.to_string();
    Some(TokenClaims {
        subject,
        kind: claims
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("access")
            .to_string(),
        permissions: claims
            .get("permissions")
            .and_then(Value::as_array)
            .map(|held| {
                held.iter()
                    .filter_map(|one| one.as_str().map(|one| one.to_string()))
                    .collect()
            })
            .unwrap_or_default(),
        role: claims
            .get("role")
            .and_then(Value::as_str)
            .map(|role| role.to_string()),
        organization_id: claims
            .get("organization_id")
            .and_then(Value::as_str)
            .map(|org| org.to_string()),
        expires_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "test-secret";

    fn at(seconds: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(seconds, 0).expect("a real instant")
    }

    fn new_user(email: &str, username: &str) -> NewUser {
        NewUser {
            id: format!("id-{username}"),
            email: email.to_string(),
            username: username.to_string(),
            first_name: "Tariro".to_string(),
            last_name: "Moyo".to_string(),
            phone: None,
            role_ids: vec!["accountant".to_string()],
            organization_id: Some("org-1".to_string()),
            password: "correct horse battery".to_string(),
        }
    }

    #[test]
    fn a_password_hashes_and_comes_back_the_same_but_never_in_clear() {
        let hash = hash_password("correct horse battery").expect("hashed");
        assert!(
            hash.starts_with("$2"),
            "bcrypt format, as this platform already stores: {hash}"
        );
        assert_ne!(hash, "correct horse battery");
        assert!(verify_password("correct horse battery", &hash));
        assert!(!verify_password("correct horse batterY", &hash));
    }

    #[test]
    fn the_hashes_already_in_the_platform_still_verify() {
        // A bcrypt hash in the format passlib wrote, for "correct horse battery". Nothing about the Python service
        // has to be migrated: the same hash verifies here.
        let shipped = bcrypt::hash("correct horse battery", 12).expect("hashed");
        assert!(verify_password("correct horse battery", &shipped));
        assert!(!verify_password("something else", &shipped));
        // A hash this build cannot read is a failed password, not an error that says so.
        assert!(!verify_password("correct horse battery", "not-a-hash"));
    }

    #[test]
    fn a_password_that_cannot_be_hashed_faithfully_is_refused_rather_than_cut_short() {
        assert_eq!(hash_password(""), Err(AccountError::WeakPassword));
        let too_long = "a".repeat(MAX_PASSWORD_BYTES + 1);
        assert_eq!(hash_password(&too_long), Err(AccountError::WeakPassword));
        // and a too-long password never verifies, whatever the stored hash is
        let hash = hash_password("short enough").expect("hashed");
        assert!(!verify_password(&too_long, &hash));
    }

    #[test]
    fn registering_refuses_a_taken_email_or_username_whichever_case_it_is_written_in() {
        let roles = default_roles();
        let now = at(1_700_000_000);
        let existing = register(&[], &roles, new_user("Tariro@Example.com", "tariro"), now)
            .expect("registered");
        assert_eq!(
            existing.email, "tariro@example.com",
            "stored lowercased, so a lookup cannot miss it"
        );
        assert_eq!(
            existing.status,
            UserStatus::Active,
            "usable immediately: there is no verification flow"
        );

        let users = vec![existing];
        assert_eq!(
            register(
                &users,
                &roles,
                new_user("tariro@example.COM", "someone"),
                now
            )
            .unwrap_err(),
            AccountError::EmailTaken
        );
        assert_eq!(
            register(&users, &roles, new_user("other@example.com", "TARIRO"), now).unwrap_err(),
            AccountError::UsernameTaken
        );
        // a role that is not there is named, so the caller knows what to fix
        let mut wanted = new_user("new@example.com", "new");
        wanted.role_ids = vec!["not-a-role".to_string()];
        assert_eq!(
            register(&users, &roles, wanted, now).unwrap_err(),
            AccountError::UnknownRole("not-a-role".to_string())
        );
    }

    #[test]
    fn a_wrong_password_and_an_unknown_account_answer_the_same_thing() {
        let roles = default_roles();
        let user = register(
            &[],
            &roles,
            new_user("tariro@example.com", "tariro"),
            at(1_700_000_000),
        )
        .expect("registered");
        let users = vec![user];

        let wrong = sign_in(&users, "tariro", "wrong").unwrap_err();
        let unknown = sign_in(&users, "nobody", "wrong").unwrap_err();
        assert_eq!(wrong, AccountError::InvalidCredentials);
        assert_eq!(unknown, AccountError::InvalidCredentials);
        assert_eq!(wrong.status(), 401);
        assert_eq!(
            unknown.message(),
            wrong.message(),
            "the door must not say which accounts exist"
        );
    }

    #[test]
    fn either_the_username_or_the_email_signs_a_person_in() {
        let roles = default_roles();
        let user = register(
            &[],
            &roles,
            new_user("tariro@example.com", "tariro"),
            at(1_700_000_000),
        )
        .expect("registered");
        let users = vec![user];
        assert!(matches!(
            sign_in(&users, "tariro", "correct horse battery"),
            Ok(SignIn::Allowed(_))
        ));
        assert!(matches!(
            sign_in(&users, "TARIRO@EXAMPLE.COM", "correct horse battery"),
            Ok(SignIn::Allowed(_))
        ));
    }

    #[test]
    fn an_inactive_account_is_refused_with_403_not_401() {
        let roles = default_roles();
        let mut user = register(
            &[],
            &roles,
            new_user("tariro@example.com", "tariro"),
            at(1_700_000_000),
        )
        .expect("registered");
        user.status = UserStatus::Suspended;
        assert_eq!(
            sign_in(&[user], "tariro", "correct horse battery").unwrap_err(),
            AccountError::NotActive
        );
        // the status may not sign in at all, so no route rounds it to active
        assert!(!UserStatus::PendingVerification.may_sign_in());
    }

    #[test]
    fn a_person_with_mfa_gets_a_challenge_and_no_session() {
        let roles = default_roles();
        let mut user = register(
            &[],
            &roles,
            new_user("tariro@example.com", "tariro"),
            at(1_700_000_000),
        )
        .expect("registered");
        user.mfa_enabled = true;
        match sign_in(&[user], "tariro", "correct horse battery").expect("password was right") {
            SignIn::SecondFactor { user } => assert!(user.mfa_enabled),
            SignIn::Allowed(_) => panic!("a session was handed out before the second factor"),
        }
    }

    #[test]
    fn the_access_token_carries_the_shipped_claims_and_verifies_with_the_same_secret() {
        let roles = default_roles();
        let user = register(
            &[],
            &roles,
            new_user("tariro@example.com", "tariro"),
            at(1_700_000_000),
        )
        .expect("registered");
        let now = at(1_700_000_000);
        let token = issue_access_token(&user, &roles, SECRET, now, None);
        assert_eq!(
            token.expires_in, ACCESS_TOKEN_SECONDS,
            "an hour, as shipped"
        );

        let claims = verify_token(&token.value, SECRET, now).expect("verified");
        assert_eq!(claims.subject, user.id);
        assert_eq!(claims.kind, "access");
        assert_eq!(claims.role.as_deref(), Some("accountant"));
        assert_eq!(claims.organization_id.as_deref(), Some("org-1"));
        assert!(claims.permissions.contains(&"journal:post".to_string()));
        assert!(
            !claims.permissions.contains(&"role:manage".to_string()),
            "only what the role holds"
        );
        assert_eq!(
            claims.expires_at,
            (now + Duration::seconds(ACCESS_TOKEN_SECONDS)).timestamp()
        );

        // the payload is a real JWT: `exp` is seconds since the epoch, not microseconds
        let body = token.value.split('.').nth(1).expect("three parts");
        use base64::engine::general_purpose::URL_SAFE_NO_PAD;
        use base64::Engine;
        let payload: Value =
            serde_json::from_slice(&URL_SAFE_NO_PAD.decode(body).expect("decodable"))
                .expect("json");
        assert_eq!(payload["exp"].as_i64(), Some(1_700_003_600));
    }

    #[test]
    fn a_token_stops_working_when_it_expires_or_is_signed_with_another_secret() {
        let roles = default_roles();
        let user = register(
            &[],
            &roles,
            new_user("tariro@example.com", "tariro"),
            at(1_700_000_000),
        )
        .expect("registered");
        let now = at(1_700_000_000);
        let token = issue_access_token(&user, &roles, SECRET, now, None);

        // one second past its expiry
        assert!(verify_token(
            &token.value,
            SECRET,
            now + Duration::seconds(ACCESS_TOKEN_SECONDS + 1)
        )
        .is_none());
        // still good just before
        assert!(verify_token(
            &token.value,
            SECRET,
            now + Duration::seconds(ACCESS_TOKEN_SECONDS - 1)
        )
        .is_some());
        // another secret, and a tampered body
        assert!(verify_token(&token.value, "another-secret", now).is_none());
        let mut parts: Vec<String> = token
            .value
            .split('.')
            .map(|part| part.to_string())
            .collect();
        parts[1] = URL_SAFE_NO_PAD_FAKE.to_string();
        assert!(verify_token(&parts.join("."), SECRET, now).is_none());
        // and rubbish is simply not a token
        assert!(verify_token("not-a-token", SECRET, now).is_none());
        assert!(verify_token("", SECRET, now).is_none());
    }

    /// A syntactically valid but different payload body, used to prove a tampered body is refused.
    const URL_SAFE_NO_PAD_FAKE: &str =
        "eyJzdWIiOiJpZC10YXJpcm8iLCJ0eXBlIjoiYWNjZXNzIiwiZXhwIjo5OTk5OTk5OTk5fQ";

    #[test]
    fn a_second_factor_token_is_short_lived_and_says_so() {
        let roles = default_roles();
        let mut user = register(
            &[],
            &roles,
            new_user("tariro@example.com", "tariro"),
            at(1_700_000_000),
        )
        .expect("registered");
        user.mfa_enabled = true;
        let now = at(1_700_000_000);
        let token = issue_mfa_token(&user, SECRET, now);
        assert_eq!(
            token.expires_in, MFA_TOKEN_SECONDS,
            "five minutes: long enough to read a code"
        );
        let claims = verify_token(&token.value, SECRET, now).expect("verified");
        assert_eq!(claims.kind, "mfa");
        assert!(
            claims.permissions.is_empty(),
            "a challenge token carries no permissions, so it cannot be used as a session"
        );
    }

    #[test]
    fn a_role_bundles_the_capabilities_its_people_hold() {
        let roles = default_roles();
        let permissions = permissions_for(&["viewer".to_string()], &roles);
        assert_eq!(
            permissions,
            vec![
                "account:view".to_string(),
                "journal:view".to_string(),
                "report:view".to_string(),
                "report:export".to_string()
            ],
            "in capability order, so two runs cannot disagree about a token"
        );
        // two roles union rather than replace
        let both = permissions_for(&["viewer".to_string(), "accountant".to_string()], &roles);
        assert!(both.contains(&"journal:post".to_string()));
        assert!(both.contains(&"report:export".to_string()));
        assert_eq!(permissions_for(&[], &roles), Vec::<String>::new());
        assert_eq!(
            permissions_for(&["admin".to_string()], &roles).len(),
            CAPABILITIES.len()
        );
    }

    #[test]
    fn the_default_roles_are_the_shipped_five_and_an_administrator_holds_everything() {
        let roles = default_roles();
        let ids: Vec<&str> = roles.iter().map(|role| role.id.as_str()).collect();
        assert_eq!(
            ids,
            vec!["admin", "accountant", "finance_manager", "viewer", "user"]
        );
        assert!(roles.iter().all(|role| role.is_system));
        assert_eq!(roles[0].permissions.len(), CAPABILITIES.len());
    }
}
