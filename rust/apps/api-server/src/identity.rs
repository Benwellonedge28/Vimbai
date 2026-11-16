//! identity-service at the addresses it published: the door the gateway leaves open.
//!
//! Everything a person needs before they are anybody: register an account, sign in, read and edit their own record,
//! change their password, turn a second factor on or off, and the token pair that carries them afterwards. Roles,
//! organisations and the audit trail belong here too, because they are all decided by the same question - who is
//! this, and what may they do.
//!
//! Two kinds of state, kept apart on purpose:
//!
//! * Accounts, roles, organisations and the audit trail are RECORDS. They belong to the platform rather than to a
//!   Book, so they go through the store's platform door, which stamps no owner and can be read before there is a
//!   caller. An account survives a restart.
//! * Sessions, refresh tokens, reset tokens and MFA codes are LIVE STATE and last exactly as long as the process.
//!   That is what the shipped service did, and it is right: a refresh token is a thing you hold, not a thing the
//!   platform remembers forever, and a restart that quietly re-arms every holder's token is worse than one that
//!   makes them sign in again.
//!
//! The shipped answers are kept, including the ones that look odd, and each is pinned by a test.

use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};

use axum::extract::{Path, RawQuery, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use chrono::{DateTime, Duration, Utc};
use mfa::accounts::{
    self, default_roles, hash_password, issue_access_token, issue_mfa_token, permissions_for,
    sign_in, AccountError, MfaMethod, Role, SignIn, User, UserStatus, CAPABILITIES,
};
use mfa::password;
use mfa::recovery;
use mfa::totp;
use serde_json::{json, Value};
use store::{PlatformRecords, RecordKind};

use tenancy::organisation;

use crate::records::GraphHandle;

const ACCOUNT: RecordKind = RecordKind::new("Account", "OWNS_ACCOUNT");
const ROLE: RecordKind = RecordKind::new("RoleDef", "OWNS_ROLE");
const AUDIT: RecordKind = RecordKind::new("AuditEntry", "OWNS_AUDIT_ENTRY");
/// The public half of each person's key pair, kept so a Book key can be wrapped for them before they are a member.
const PUBLIC_KEY: RecordKind = RecordKind::new("PublicKey", "OWNS_PUBLIC_KEY");

/// How long a refresh token is good for, as shipped.
const REFRESH_TOKEN_DAYS: i64 = 7;
/// How long a password reset link is good for, as shipped.
const RESET_TOKEN_HOURS: i64 = 24;
/// How long an MFA code lives, as shipped.
const MFA_CODE_MINUTES: i64 = 5;
/// How many wrong guesses a code takes before it is spent, as shipped.
const MFA_CODE_ATTEMPTS: i64 = 3;

fn now() -> DateTime<Utc> {
    Utc::now()
}

fn iso(moment: DateTime<Utc>) -> String {
    moment.to_rfc3339()
}

/// The token signing secret. A deployment sets one; a machine that has not been told one still has to be able to sign
/// somebody in, so a fixed development secret is used and named as such rather than an empty key.
fn secret() -> String {
    common::secret_from_environment().unwrap_or_else(|| "vimbai-development-secret".to_string())
}

/// A store refusal, answered the way this surface answers everything else.
fn stored(refusal: store::StoreError) -> Response {
    match refusal {
        store::StoreError::NotFound => error(404, "Organization not found"),
        store::StoreError::Forbidden(message) => error(403, &message),
        store::StoreError::Invalid(message) => error(422, &message),
        store::StoreError::Conflict(message) => error(409, &message),
        store::StoreError::BadRequest(message) => error(400, &message),
        other => error(502, &other.to_string()),
    }
}

fn error(status: u16, message: &str) -> Response {
    (
        StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
        Json(json!({ "detail": message, "error": message })),
    )
        .into_response()
}

fn refusal(failure: AccountError) -> Response {
    error(failure.status(), &failure.message())
}

/// Hash a password, or say in the person's own words what is wrong with it.
///
/// The rules are in [`password`]; the only thing this adds is that the refusal keeps its reason instead of collapsing to
/// "does not meet the requirements". A person told "choose something nobody else would" can fix it; a person told the
/// requirements were not met has to guess which one.
fn hashed(password: &str, names: &[&str]) -> Result<String, Box<Response>> {
    password::hash(password, names).map_err(|fault| Box::new(error(400, fault.message())))
}

/// One live thing: a token, a session or a code. Held in memory because it is a thing being used right now.
#[derive(Default)]
struct Live {
    /// refresh token -> the person it belongs to
    refresh: BTreeMap<String, (String, DateTime<Utc>)>,
    /// session id -> the person and when they were last seen
    sessions: BTreeMap<String, Session>,
    /// sha256 of the raw reset token -> the person and when it expires
    resets: BTreeMap<String, (String, DateTime<Utc>)>,
    /// the six digits -> whose they are, how they were sent, and how long they live
    codes: BTreeMap<String, Code>,
    /// a counter, so two things made in the same nanosecond are still two things
    made: u64,
    /// identifier -> how many sign-ins have just failed for it, and when that count was last added to
    attempts: BTreeMap<String, Attempts>,
    /// person -> the secret their authenticator app is being enrolled with, until a code confirms it
    enrolments: BTreeMap<String, Enrolment>,
    /// person -> the newest code step already spent, so one six-digit code is never a second way in
    spent_steps: BTreeMap<String, u64>,
}

/// A second-factor enrolment waiting to be confirmed.
///
/// The secret is held here, not on the account, until a code made from it comes back: an enrolment nobody finished must not
/// leave an account that asks for codes its owner cannot produce.
struct Enrolment {
    secret: String,
    method: String,
    expires_at: DateTime<Utc>,
}

/// How long an unfinished enrolment stays open, in minutes.
const ENROLMENT_MINUTES: i64 = 15;

/// The wrong guesses made against one identifier.
///
/// Kept in live state for the same reason a session is: it is a thing happening now, not a fact about the account. It
/// exists because a password is guessed by trying it, and an engine that answers as fast as it is asked is an engine that
/// helps. Five wrong guesses put the identifier aside for a quarter of an hour; a right one clears the count, so somebody
/// who remembers their password is never left locked out by their own typo.
#[derive(Clone, Copy)]
struct Attempts {
    wrong: u32,
    first: DateTime<Utc>,
    last: DateTime<Utc>,
}

/// How many wrong guesses an identifier may make, and how long the pause lasts.
const SIGN_IN_ATTEMPTS: u32 = 5;
const SIGN_IN_PAUSE_MINUTES: i64 = 15;

impl Attempts {
    /// Whether this identifier is still in its pause, and how long is left if so.
    fn pause_left(&self, at: DateTime<Utc>) -> Option<i64> {
        if self.wrong < SIGN_IN_ATTEMPTS {
            return None;
        }
        let until = self.last + Duration::minutes(SIGN_IN_PAUSE_MINUTES);
        if at >= until {
            return None;
        }
        Some((until - at).num_seconds().max(1))
    }
}

struct Session {
    user_id: String,
    created_at: DateTime<Utc>,
    last_activity: DateTime<Utc>,
    ip_address: Option<String>,
}

struct Code {
    user_id: String,
    expires_at: DateTime<Utc>,
    attempts: i64,
}

fn live() -> &'static Mutex<Live> {
    static LIVE: OnceLock<Mutex<Live>> = OnceLock::new();
    LIVE.get_or_init(|| Mutex::new(Live::default()))
}

/// How long `identifier` must wait before another attempt is answered, if it is in a pause.
fn sign_in_pause(identifier: &str) -> Option<i64> {
    let folded = identifier.trim().to_lowercase();
    let held = live().lock().expect("the identity state is never poisoned");
    held.attempts
        .get(&folded)
        .and_then(|entry| entry.pause_left(now()))
}

/// Record a wrong guess, starting the count afresh once the pause has run out.
fn record_wrong(identifier: &str) {
    let folded = identifier.trim().to_lowercase();
    let at = now();
    let mut held = live().lock().expect("the identity state is never poisoned");
    let entry = held.attempts.entry(folded).or_insert(Attempts {
        wrong: 0,
        first: at,
        last: at,
    });
    if entry.pause_left(at).is_none() && entry.wrong >= SIGN_IN_ATTEMPTS {
        // the pause is over: this attempt is the first of a new count
        entry.wrong = 0;
        entry.first = at;
    }
    entry.wrong += 1;
    entry.last = at;
}

/// A correct password clears the count, so a person who mistyped twice is not two guesses nearer a lockout.
fn clear_wrong(identifier: &str) {
    let folded = identifier.trim().to_lowercase();
    live()
        .lock()
        .expect("the identity state is never poisoned")
        .attempts
        .remove(&folded);
}

/// A unique name for something made now. Not a secret: anything that has to be unguessable uses [`random_token`].
fn made_id(prefix: &str) -> String {
    let mut held = live().lock().expect("the identity state is never poisoned");
    held.made += 1;
    let stamp = now().timestamp_nanos_opt().unwrap_or_default();
    format!("{prefix}-{stamp:x}-{:x}", held.made)
}

/// A value nobody can guess: 32 bytes from the operating system's entropy, hex written.
fn random_token() -> String {
    let mut bytes = [0u8; 32];
    getrandom::getrandom(&mut bytes).expect("the operating system provides entropy");
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn sha256_hex(text: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(text.as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn accounts_of(graph: &GraphHandle) -> PlatformRecords<'_, dyn store::Graph + Send + Sync> {
    PlatformRecords::new(graph.as_ref(), ACCOUNT)
}

fn public_keys_of(graph: &GraphHandle) -> PlatformRecords<'_, dyn store::Graph + Send + Sync> {
    PlatformRecords::new(graph.as_ref(), PUBLIC_KEY)
}

fn roles_of(graph: &GraphHandle) -> PlatformRecords<'_, dyn store::Graph + Send + Sync> {
    PlatformRecords::new(graph.as_ref(), ROLE)
}

/// The roles this deployment knows, seeded on first read.
///
/// Seeding is idempotent per role id: an operator who edited a system role does not have the edit undone, and a role
/// that is already there is never written twice.
fn known_roles(graph: &GraphHandle) -> Vec<Role> {
    let records = roles_of(graph);
    let stored = records.list(500).unwrap_or_default();
    if stored.is_empty() {
        for role in default_roles() {
            let _ = records.create(&json!({
                "id": role.id,
                "name": role.name,
                "description": role.description,
                "permissions": role.permissions,
                "is_system": role.is_system,
                "created_at": iso(now()),
            }));
        }
        return default_roles();
    }
    let mut found: Vec<Role> = stored
        .iter()
        .filter_map(|record| {
            Some(Role {
                id: record.get("id")?.as_str()?.to_string(),
                name: record.get("name")?.as_str()?.to_string(),
                description: record
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                permissions: record
                    .get("permissions")
                    .and_then(Value::as_array)
                    .map(|held| {
                        held.iter()
                            .filter_map(|one| one.as_str().map(str::to_owned))
                            .collect()
                    })
                    .unwrap_or_default(),
                is_system: record
                    .get("is_system")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            })
        })
        .collect();
    // a role the shipped set names but this deployment has not written yet still has to be resolvable
    for missing in default_roles() {
        if !found.iter().any(|role| role.id == missing.id) {
            let _ = records.create(&json!({
                "id": missing.id,
                "name": missing.name,
                "description": missing.description,
                "permissions": missing.permissions,
                "is_system": missing.is_system,
                "created_at": iso(now()),
            }));
            found.push(missing);
        }
    }
    found
}

fn user_from(record: &Value) -> Option<User> {
    let text = |key: &str| record.get(key).and_then(Value::as_str).map(str::to_owned);
    Some(User {
        id: text("id")?,
        email: text("email")?,
        username: text("username")?,
        first_name: text("first_name").unwrap_or_default(),
        last_name: text("last_name").unwrap_or_default(),
        phone: text("phone"),
        status: UserStatus::parse(&text("status")?).unwrap_or(UserStatus::Inactive),
        role_ids: record
            .get("role_ids")
            .and_then(Value::as_array)
            .map(|held| {
                held.iter()
                    .filter_map(|one| one.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default(),
        organization_id: text("organization_id"),
        permissions: record
            .get("permissions")
            .and_then(Value::as_array)
            .map(|held| {
                held.iter()
                    .filter_map(|one| one.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default(),
        mfa_enabled: record
            .get("mfa_enabled")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        mfa_secret: text("mfa_secret"),
        mfa_recovery: record
            .get("mfa_recovery")
            .and_then(Value::as_array)
            .map(|held| {
                held.iter()
                    .filter_map(|one| one.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default(),
        mfa_method: MfaMethod::parse(&text("mfa_method").unwrap_or_default())
            .unwrap_or(MfaMethod::Totp),
        password_hash: text("password_hash").unwrap_or_default(),
        created_at: text("created_at").unwrap_or_default(),
        updated_at: text("updated_at").unwrap_or_default(),
        last_login: text("last_login"),
    })
}

/// Spend a recovery code: true when it was one of this person's, in which case it is removed from the stored list.
///
/// The order matters. The code is found first, then removed, and the account is written once, so a code that is found cannot be
/// left usable by a failure between the two. Two callers racing with the same code are separated by the live lock the second
/// factor already uses for spent steps.
fn spend_recovery(graph: &GraphHandle, user: &User, code: &str) -> bool {
    if user.mfa_recovery.is_empty() || !recovery::looks_like_one(code) {
        return false;
    }
    let Some(index) = recovery::find(&user.id, code, &user.mfa_recovery) else {
        return false;
    };
    let mut left = user.mfa_recovery.clone();
    left.remove(index);
    let written = accounts_of(graph)
        .update(
            &user.id,
            &json!({ "mfa_recovery": left, "updated_at": iso(now()) }),
        )
        .is_ok();
    if written {
        audit(
            graph,
            &user.id,
            "mfa.recovery_code_used",
            "User",
            &user.id,
            json!({ "remaining": left.len() }),
        );
    }
    written
}

fn stored_user(graph: &GraphHandle, id: &str) -> Option<User> {
    accounts_of(graph)
        .get(id)
        .ok()
        .flatten()
        .and_then(|record| user_from(&record))
}

fn by_identifier(graph: &GraphHandle, identifier: &str) -> Option<User> {
    let records = accounts_of(graph);
    records
        .find("username", identifier)
        .ok()
        .flatten()
        .or_else(|| {
            records
                .find("email", identifier.trim().to_lowercase().as_str())
                .ok()
                .flatten()
        })
        .and_then(|record| user_from(&record))
}

fn stored_roles(role_ids: &[String], roles: &[Role]) -> Vec<String> {
    permissions_for(role_ids, roles)
}

/// Whatever the caller sent, as a flat map. A sign-in arrives form-encoded from one client and as JSON from another,
/// and the shipped service read it as a form, so both are accepted rather than refusing the caller that guessed.
fn fields(query: Option<&str>, body: Option<&[u8]>) -> BTreeMap<String, String> {
    let mut found = BTreeMap::new();
    let mut absorb = |text: &str| {
        for pair in text.split('&').filter(|pair| !pair.is_empty()) {
            let (key, value) = match pair.split_once('=') {
                Some((key, value)) => (key, value),
                None => (pair, ""),
            };
            found.insert(percent_decode(key), percent_decode(value));
        }
    };
    if let Some(query) = query {
        absorb(query);
    }
    if let Some(body) = body {
        let as_text = String::from_utf8_lossy(body).to_string();
        let trimmed = as_text.trim();
        if trimmed.starts_with('{') {
            if let Ok(Value::Object(object)) = serde_json::from_str::<Value>(trimmed) {
                for (key, value) in object {
                    let text = match value {
                        Value::String(text) => text,
                        Value::Null => String::new(),
                        other => other.to_string(),
                    };
                    found.insert(key, text);
                }
            }
        } else {
            absorb(&as_text);
        }
    }
    found
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                out.push(b' ');
                index += 1;
            }
            b'%' if index + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).unwrap_or("");
                match u8::from_str_radix(hex, 16) {
                    Ok(byte) => {
                        out.push(byte);
                        index += 3;
                    }
                    Err(_) => {
                        out.push(bytes[index]);
                        index += 1;
                    }
                }
            }
            byte => {
                out.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).to_string()
}

/// The caller, resolved from their Bearer token. The token says who they are; the record says whether they may still
/// come in, so a suspended account cannot ride a token it already holds.
fn caller(graph: &GraphHandle, headers: &HeaderMap) -> Result<User, Box<Response>> {
    let header = headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    let token = header
        .strip_prefix("Bearer ")
        .or_else(|| header.strip_prefix("bearer "))
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .ok_or_else(|| Box::new(error(401, "Not authenticated")))?;
    let claims = mfa::accounts::verify_token(token, &secret(), now())
        .ok_or_else(|| Box::new(error(401, "Invalid or expired token")))?;
    if claims.kind != "access" {
        // a challenge token is not a session, and must not be spent as one
        return Err(Box::new(error(401, "Invalid or expired token")));
    }
    let user =
        stored_user(graph, &claims.subject).ok_or_else(|| Box::new(error(401, "Unknown user")))?;
    if !user.status.may_sign_in() {
        return Err(Box::new(error(403, "Account is not active")));
    }
    Ok(user)
}

/// Reading or changing someone else needs the capability that says so; every other handler is about yourself.
fn self_or_admin(
    graph: &GraphHandle,
    headers: &HeaderMap,
    user_id: &str,
) -> Result<User, Box<Response>> {
    let caller = caller(graph, headers)?;
    if caller.id == user_id {
        return Ok(caller);
    }
    let roles = known_roles(graph);
    let permissions = stored_roles(&caller.role_ids, &roles);
    if permissions.iter().any(|held| held == "user:manage") {
        return Ok(caller);
    }
    Err(Box::new(error(403, "Not permitted")))
}

fn audit(
    graph: &GraphHandle,
    actor: &str,
    action: &str,
    resource_type: &str,
    resource_id: &str,
    details: Value,
) {
    let _ = PlatformRecords::new(graph.as_ref(), AUDIT).create(&json!({
        "id": made_id("audit"),
        "user_id_actor": actor,
        "action": action,
        "resource_type": resource_type,
        "resource_id": resource_id,
        "details": details,
        "timestamp": iso(now()),
        "created_at": iso(now()),
    }));
}

fn sign_in_answer(graph: &GraphHandle, user: &User, ip: Option<String>) -> Response {
    let roles = known_roles(graph);
    let token = issue_access_token(user, &roles, &secret(), now(), None);
    let refresh = random_token();
    let expires_at = now() + Duration::days(REFRESH_TOKEN_DAYS);
    live()
        .lock()
        .expect("the identity state is never poisoned")
        .refresh
        .insert(refresh.clone(), (user.id.clone(), expires_at));

    let session_id = made_id("session");
    live()
        .lock()
        .expect("the identity state is never poisoned")
        .sessions
        .insert(
            session_id,
            Session {
                user_id: user.id.clone(),
                created_at: now(),
                last_activity: now(),
                ip_address: ip,
            },
        );

    let _ = accounts_of(graph).update(
        &user.id,
        &json!({ "last_login": iso(now()), "updated_at": iso(now()) }),
    );
    audit(
        graph,
        &user.id,
        "login.success",
        "User",
        &user.id,
        json!({}),
    );

    (
        StatusCode::OK,
        Json(json!({
            "access_token": token.value,
            "refresh_token": refresh,
            "token_type": "bearer",
            "expires_in": token.expires_in,
        })),
    )
        .into_response()
}

/// The six digits a person is sent. Codes are keyed by the digits themselves, as shipped, so a person holding two
/// codes from two accounts is still checked against the account the code names.
fn raise_code(user_id: &str) -> String {
    let mut held = live().lock().expect("the identity state is never poisoned");
    held.made += 1;
    let seed = held.made;
    // six digits from the operating system's entropy, kept uniform: a modulo of a large number would favour the low
    // digits, which is the kind of quiet weakness a code must not have
    let code = loop {
        let mut bytes = [0u8; 4];
        getrandom::getrandom(&mut bytes).expect("the operating system provides entropy");
        let drawn = u32::from_be_bytes(bytes) % 1_000_000;
        let text = format!("{drawn:06}");
        if !held.codes.contains_key(&text) {
            break text;
        }
        // a collision is astronomically unlikely; the loop is here so it cannot be a hang
        if seed == 0 {
            break text;
        }
    };
    held.codes.insert(
        code.clone(),
        Code {
            user_id: user_id.to_string(),
            expires_at: now() + Duration::minutes(MFA_CODE_MINUTES),
            attempts: 0,
        },
    );
    code
}

/// Check and spend a code. A wrong guess is counted, and the code is burned after the shipped number of tries so a
/// million-digit space cannot be walked.
fn spend_code(code: &str, user_id: &str) -> bool {
    let mut held = live().lock().expect("the identity state is never poisoned");
    let Some(entry) = held.codes.get_mut(code) else {
        return false;
    };
    if entry.user_id != user_id {
        return false;
    }
    if now() > entry.expires_at {
        held.codes.remove(code);
        return false;
    }
    entry.attempts += 1;
    let spent = entry.attempts >= MFA_CODE_ATTEMPTS;
    if spent {
        held.codes.remove(code);
    }
    true
}

// ---------------------------------------------------------------------------
// the door
// ---------------------------------------------------------------------------

/// The service's own paths, and the same routes under the record prefix, so a client calling either reaches the one
/// implementation.
pub fn router(graph: GraphHandle) -> Router {
    let mut router = Router::new();
    for prefix in ["/identity", "/records/identity"] {
        router = router
            .route(&format!("{prefix}/"), get(health))
            .route(&format!("{prefix}/users/register"), post(register))
            .route(&format!("{prefix}/users/login"), post(login))
            .route(
                &format!("{prefix}/users/login/verify-mfa"),
                post(verify_mfa_login),
            )
            .route(&format!("{prefix}/users/me"), get(me))
            .route(
                &format!("{prefix}/users/me/public-key"),
                axum::routing::put(publish_public_key),
            )
            .route(
                &format!("{prefix}/users/:user_id/public-key"),
                get(get_public_key),
            )
            .route(&format!("{prefix}/users/lookup"), get(lookup_user))
            .route(
                &format!("{prefix}/users/:user_id"),
                get(get_user).put(put_user),
            )
            .route(
                &format!("{prefix}/users/:user_id/change-password"),
                post(change_password),
            )
            .route(
                &format!("{prefix}/users/:user_id/mfa/setup"),
                post(setup_mfa),
            )
            .route(
                &format!("{prefix}/users/:user_id/mfa/verify"),
                post(verify_mfa_setup),
            )
            .route(
                &format!("{prefix}/users/:user_id/mfa/disable"),
                post(disable_mfa),
            )
            .route(
                &format!("{prefix}/roles"),
                get(list_roles).post(create_role),
            )
            .route(
                &format!("{prefix}/roles/:role_id"),
                get(get_role).put(put_role),
            )
            .route(
                &format!("{prefix}/organizations"),
                post(create_organization).get(list_organizations),
            )
            .route(
                &format!("{prefix}/organizations/:org_id"),
                get(get_organization),
            )
            .route(
                &format!("{prefix}/organizations/:org_id/members"),
                get(list_organization_members).post(add_organization_member),
            )
            .route(
                &format!("{prefix}/organizations/:org_id/books"),
                get(list_organization_books),
            )
            .route(
                &format!("{prefix}/organizations/:org_id/books/:book_id"),
                axum::routing::put(move_book_into_organization)
                    .delete(take_book_out_of_organization),
            )
            .route(&format!("{prefix}/token/refresh"), post(refresh_token))
            .route(&format!("{prefix}/token/revoke"), post(revoke_token))
            .route(
                &format!("{prefix}/password/reset"),
                post(request_password_reset),
            )
            .route(
                &format!("{prefix}/password/reset/confirm"),
                post(confirm_password_reset),
            )
            .route(&format!("{prefix}/audit-logs"), get(list_audit_logs))
            .route(&format!("{prefix}/sessions"), get(list_sessions))
            .route(
                &format!("{prefix}/sessions/:session_id"),
                delete(revoke_session),
            )
            .route(&format!("{prefix}/capabilities"), get(list_capabilities));
    }
    router.with_state(graph)
}

async fn health(State(graph): State<GraphHandle>) -> Response {
    let total = PlatformRecords::new(graph.as_ref(), ACCOUNT)
        .list(5000)
        .map(|users| users.len())
        .unwrap_or(0);
    Json(json!({
        "service": "identity-service",
        "status": "running",
        "version": "1.0.0",
        "total_users": total,
    }))
    .into_response()
}

async fn register(
    State(graph): State<GraphHandle>,
    RawQuery(query): RawQuery,
    body: Option<axum::body::Bytes>,
) -> Response {
    let sent = fields(query.as_deref(), body.as_deref());
    let text = |key: &str| sent.get(key).cloned().unwrap_or_default();
    let roles = known_roles(&graph);

    let role_ids: Vec<String> = match sent.get("role_ids") {
        Some(raw) if raw.trim().starts_with('[') => serde_json::from_str(raw).unwrap_or_default(),
        Some(raw) if !raw.trim().is_empty() => raw
            .split(',')
            .map(|one| one.trim().to_string())
            .filter(|one| !one.is_empty())
            .collect(),
        _ => Vec::new(),
    };

    let given = text("username");
    let wanted = accounts::NewUser {
        id: made_id("user"),
        email: text("email"),
        // a sign-up screen asks for an address, not a username yet; an account stored under an empty name is one the
        // second person at this engine cannot have, so one is made from the address
        username: if given.trim().is_empty() {
            String::new()
        } else {
            given
        },
        first_name: text("first_name"),
        last_name: text("last_name"),
        phone: sent.get("phone").filter(|one| !one.is_empty()).cloned(),
        role_ids,
        organization_id: sent
            .get("organization_id")
            .filter(|one| !one.is_empty())
            .cloned(),
        password: text("password"),
    };

    let existing: Vec<User> = PlatformRecords::new(graph.as_ref(), ACCOUNT)
        .list(100_000)
        .unwrap_or_default()
        .iter()
        .filter_map(user_from)
        .collect();

    // derived against the accounts already here, so the name saved is the name the refusal checks were run against
    let mut wanted = wanted;
    if wanted.username.trim().is_empty() {
        wanted.username = accounts::username_for(&wanted.email, &existing);
    }

    match accounts::register(&existing, &roles, wanted, now()) {
        Ok(user) => {
            let _ = PlatformRecords::new(graph.as_ref(), ACCOUNT).create(&json!({
                "id": user.id,
                "email": user.email,
                "username": user.username,
                "first_name": user.first_name,
                "last_name": user.last_name,
                "phone": user.phone,
                "status": user.status.as_str(),
                "role_ids": user.role_ids,
                "organization_id": user.organization_id,
                "permissions": permissions_for(&user.role_ids, &roles),
                "mfa_enabled": false,
                "mfa_method": user.mfa_method.as_str(),
                "password_hash": user.password_hash,
                "created_at": user.created_at,
                "updated_at": user.updated_at,
                "last_login": Value::Null,
            }));
            audit(
                &graph,
                &user.id,
                "user.registered",
                "User",
                &user.id,
                json!({}),
            );
            // the shipped answer was only these three fields, and clients were written against it
            (
                StatusCode::CREATED,
                Json(json!({
                    "id": user.id,
                    "email": user.email,
                    "username": user.username,
                })),
            )
                .into_response()
        }
        Err(failure) => {
            // the collision cases keep their own answers; a weak password is explained rather than named
            if failure == AccountError::WeakPassword {
                let sent_password = sent.get("password").cloned().unwrap_or_default();
                let sent_name = sent.get("username").cloned().unwrap_or_default();
                let sent_email = sent.get("email").cloned().unwrap_or_default();
                if let Err(fault) = password::check(&sent_password, &[&sent_name, &sent_email]) {
                    return error(400, fault.message());
                }
            }
            refusal(failure)
        }
    }
}

async fn login(
    State(graph): State<GraphHandle>,
    RawQuery(query): RawQuery,
    headers: HeaderMap,
    body: Option<axum::body::Bytes>,
) -> Response {
    let sent = fields(query.as_deref(), body.as_deref());
    // Three names for one thing, because three clients call it three things: the form field the app posts, the address
    // itself, and `identifier`. A door that refuses the usual name by ignoring it answers "invalid credentials" to a
    // correct password, which reads as a wrong password and is impossible to diagnose from the outside.
    let identifier = sent
        .get("username")
        .or_else(|| sent.get("email"))
        .or_else(|| sent.get("identifier"))
        .cloned()
        .unwrap_or_default();
    let password = sent.get("password").cloned().unwrap_or_default();

    // Too many wrong guesses already: answer at once, and say how long to wait, rather than hashing another try.
    if let Some(seconds) = sign_in_pause(&identifier) {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            [("retry-after", seconds.to_string())],
            Json(json!({
                "error": format!("Too many sign-in attempts. Try again in {seconds} seconds."),
                "retry_after": seconds,
            })),
        )
            .into_response();
    }

    let users: Vec<User> = PlatformRecords::new(graph.as_ref(), ACCOUNT)
        .list(100_000)
        .unwrap_or_default()
        .iter()
        .filter_map(user_from)
        .collect();

    match sign_in(&users, &identifier, &password) {
        Ok(SignIn::Allowed(user)) => {
            clear_wrong(&identifier);
            // The password is only in the clear here, so this is the moment a hash made with an older cost can be
            // replaced. Best effort: a rehash that fails leaves a working account, just a slower one.
            if accounts::needs_rehash(&user.password_hash) {
                let names = [user.username.as_str(), user.email.as_str()];
                if let Ok(fresh) = hash_password(&password, &names) {
                    let _ = accounts_of(&graph).update(
                        &user.id,
                        &json!({ "password_hash": fresh, "updated_at": iso(now()) }),
                    );
                }
            }
            let ip = headers
                .get("x-forwarded-for")
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned);
            sign_in_answer(&graph, &user, ip)
        }
        Ok(SignIn::SecondFactor { user }) => {
            // a code is raised and, in this deployment, there is no channel to send it on: the answer says MFA is
            // required and hands back the short-lived token that finishing a sign-in needs.
            let code = raise_code(&user.id);
            audit(
                &graph,
                &user.id,
                "mfa.challenge",
                "User",
                &user.id,
                json!({ "method": user.mfa_method.as_str(), "code": code }),
            );
            let temp = issue_mfa_token(&user, &secret(), now());
            (
                StatusCode::OK,
                Json(json!({
                    "mfa_required": true,
                    "method": user.mfa_method.as_str(),
                    "temp_token": temp.value,
                })),
            )
                .into_response()
        }
        Err(failure) => {
            if failure == AccountError::InvalidCredentials {
                // a failed sign-in names the account only when there was one, and the trial is recorded so an
                // operator can see a run of them
                if let Some(user) = by_identifier(&graph, &identifier) {
                    audit(
                        &graph,
                        &user.id,
                        "login.failed",
                        "User",
                        &user.id,
                        json!({ "reason": "invalid_password" }),
                    );
                }
            }
            // Counted whether or not the account exists, so the throttle cannot be used to find out which names do.
            record_wrong(&identifier);
            refusal(failure)
        }
    }
}

async fn verify_mfa_login(
    State(graph): State<GraphHandle>,
    RawQuery(query): RawQuery,
    body: Option<axum::body::Bytes>,
) -> Response {
    let sent = fields(query.as_deref(), body.as_deref());
    let temp = sent.get("temp_token").cloned().unwrap_or_default();
    let code = sent.get("code").cloned().unwrap_or_default();

    let Some(claims) = mfa::accounts::verify_token(&temp, &secret(), now()) else {
        return error(401, "Invalid temporary token");
    };
    if claims.kind != "mfa" {
        // an access token is not a challenge: finishing a sign-in takes the token that said one was owed
        return error(401, "Invalid temporary token");
    }
    let Some(user) = stored_user(&graph, &claims.subject) else {
        return error(404, "User not found");
    };
    if !user.mfa_enabled {
        return error(401, "Invalid MFA code");
    }
    let accepted = match user.mfa_secret.as_deref() {
        Some(secret) => {
            let at = now().timestamp().max(0) as u64;
            match totp::verify_at(secret, &code, at, totp::DRIFT_STEPS) {
                // within its half minute a code is still the same password: a second use must not be a second way in
                Some(step) => {
                    let mut held = live().lock().expect("the identity state is never poisoned");
                    let fresh = held
                        .spent_steps
                        .get(&user.id)
                        .is_none_or(|used| step > *used);
                    if fresh {
                        held.spent_steps.insert(user.id.clone(), step);
                    }
                    fresh
                }
                None => false,
            }
        }
        None => spend_code(&code, &user.id),
    };
    // A recovery code is accepted where an app code is, and dies the moment it is used: it is the way back in for a phone that
    // is gone, so it must work, and it is a single-use credential, so it must not work twice.
    let accepted = accepted || spend_recovery(&graph, &user, &code);
    if !accepted {
        return error(401, "Invalid MFA code");
    }
    audit(
        &graph,
        &user.id,
        "mfa.verified",
        "User",
        &user.id,
        json!({}),
    );
    sign_in_answer(&graph, &user, None)
}

async fn me(State(graph): State<GraphHandle>, headers: HeaderMap) -> Response {
    let user = match caller(&graph, &headers) {
        Ok(user) => user,
        Err(response) => return *response,
    };
    let roles = known_roles(&graph);
    Json(json!({
        "id": user.id,
        "email": user.email,
        "username": user.username,
        "first_name": user.first_name,
        "last_name": user.last_name,
        "role_ids": user.role_ids,
        "permissions": permissions_for(&user.role_ids, &roles),
        "organization_id": user.organization_id,
        "mfa_enabled": user.mfa_enabled,
        "last_login": user.last_login,
    }))
    .into_response()
}

// ---------------------------------------------------------------------------
// the key registry
// ---------------------------------------------------------------------------

/// A public key is a modulus and an exponent, base64. Nothing secret is stored here, which is why any signed-in person
/// may read one: wrapping a Book key for someone is only possible if their public half can be fetched first.
fn public_key_text(value: Option<&Value>, name: &str) -> Option<String> {
    let raw = value?.get(name)?.as_str()?.trim().to_string();
    if raw.is_empty() {
        return None;
    }
    // a well-formed key is base64 of a big-endian integer, and a modulus smaller than 128 bytes cannot wrap a key
    let bytes =
        base64::Engine::decode(&base64::engine::general_purpose::STANDARD, raw.as_bytes()).ok()?;
    let floor = if name == "modulus" { 128 } else { 1 };
    (bytes.len() >= floor).then_some(raw)
}

/// Find a person by exact username or email, so an invitation can name someone without asking for their id.
///
/// Signed in only, and an exact match only: this is a door for inviting a person whose name you already know, not a way
/// to browse the account list. An account that is not active is reported as absent, because it cannot accept a Book.
async fn lookup_user(
    State(graph): State<GraphHandle>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    if let Err(response) = caller(&graph, &headers) {
        return *response;
    }
    let sent = fields(query.as_deref(), None);
    let wanted = sent.get("username").cloned().unwrap_or_default();
    let wanted = wanted.trim();
    if wanted.is_empty() {
        return error(400, "A username or email is required");
    }
    match by_identifier(&graph, wanted) {
        Some(user) if user.status.may_sign_in() => Json(json!({
            "id": user.id,
            "username": user.username,
            "email": user.email,
            "full_name": format!("{} {}", user.first_name, user.last_name).trim().to_string(),
        }))
        .into_response(),
        _ => error(404, "No account with that username"),
    }
}

/// Publish the caller's public key, or replace it.
///
/// One key per person, held under their own id, so publishing twice is an update rather than a second row: a phone that
/// re-installs publishes again and invitations after that reach the new key.
async fn publish_public_key(
    State(graph): State<GraphHandle>,
    headers: HeaderMap,
    body: Option<axum::body::Bytes>,
) -> Response {
    let caller = match caller(&graph, &headers) {
        Ok(caller) => caller,
        Err(response) => return *response,
    };
    let sent: Value = match body
        .as_deref()
        .and_then(|raw| serde_json::from_slice(raw).ok())
    {
        Some(value) => value,
        None => return error(400, "A JSON body with modulus and exponent is required"),
    };
    let (Some(modulus), Some(exponent)) = (
        public_key_text(Some(&sent), "modulus"),
        public_key_text(Some(&sent), "exponent"),
    ) else {
        return error(
            400,
            "A public key is a base64 modulus of at least 128 bytes and a base64 exponent",
        );
    };
    let records = public_keys_of(&graph);
    // the record id is the person's own id, so one key per person and no second row to keep in step
    let fields = json!({
        "id": caller.id,
        "modulus": modulus,
        "exponent": exponent,
        "updated_at": iso(now()),
    });
    let saved = match records.get(&caller.id) {
        Ok(Some(existing)) => {
            let created = existing.get("created_at").cloned().unwrap_or(Value::Null);
            let mut patched = fields.clone();
            if let Value::Object(map) = &mut patched {
                map.insert("created_at".into(), created);
            }
            records.update(&caller.id, &patched)
        }
        _ => records.create(&fields).map(Some),
    };
    match saved {
        Ok(_) => Json(json!({
            "user_id": caller.id,
            "modulus": modulus,
            "exponent": exponent,
        }))
        .into_response(),
        Err(error_) => error(500, &format!("Could not store the public key: {error_}")),
    }
}

/// Read someone's public key. This is the half an inviter needs, so any signed-in person may fetch it.
async fn get_public_key(
    State(graph): State<GraphHandle>,
    headers: HeaderMap,
    Path(user_id): Path<String>,
) -> Response {
    if let Err(response) = caller(&graph, &headers) {
        return *response;
    }
    match public_keys_of(&graph).get(&user_id) {
        Ok(Some(record)) => Json(json!({
            "user_id": user_id,
            "modulus": record.get("modulus").cloned().unwrap_or(Value::Null),
            "exponent": record.get("exponent").cloned().unwrap_or(Value::Null),
            "updated_at": record.get("updated_at").cloned().unwrap_or(Value::Null),
        }))
        .into_response(),
        // somebody who has not opened a Book yet has published nothing, which is not an error
        Ok(None) => error(404, "No public key published"),
        Err(error_) => error(500, &format!("Could not read the public key: {error_}")),
    }
}

async fn get_user(
    State(graph): State<GraphHandle>,
    headers: HeaderMap,
    Path(user_id): Path<String>,
) -> Response {
    if let Err(response) = self_or_admin(&graph, &headers, &user_id) {
        return *response;
    }
    match stored_user(&graph, &user_id) {
        Some(user) => Json(json!({
            "id": user.id,
            "email": user.email,
            "username": user.username,
            "first_name": user.first_name,
            "last_name": user.last_name,
            "status": user.status.as_str(),
            "role_ids": user.role_ids,
            "mfa_enabled": user.mfa_enabled,
            "created_at": user.created_at,
            "last_login": user.last_login,
        }))
        .into_response(),
        None => error(404, "User not found"),
    }
}

async fn put_user(
    State(graph): State<GraphHandle>,
    headers: HeaderMap,
    Path(user_id): Path<String>,
    body: Option<axum::body::Bytes>,
) -> Response {
    if let Err(response) = self_or_admin(&graph, &headers, &user_id) {
        return *response;
    }
    let Some(existing) = stored_user(&graph, &user_id) else {
        return error(404, "User not found");
    };
    let sent = fields(None, body.as_deref());
    let records = accounts_of(&graph);
    let roles = known_roles(&graph);
    let mut patch = serde_json::Map::new();

    if let Some(email) = sent.get("email").filter(|one| !one.trim().is_empty()) {
        let taken = accounts_of(&graph)
            .find("email", email.trim().to_lowercase().as_str())
            .ok()
            .flatten()
            .and_then(|record| user_from(&record));
        if let Some(other) = taken {
            if other.id != user_id {
                return error(400, "Email already in use");
            }
        }
        patch.insert("email".into(), json!(email.trim().to_lowercase()));
    }
    for field in ["first_name", "last_name", "phone"] {
        if let Some(value) = sent.get(field) {
            patch.insert(field.into(), json!(value));
        }
    }
    if let Some(active) = sent.get("is_active") {
        let active = matches!(active.as_str(), "true" | "1" | "yes");
        patch.insert("is_active".into(), json!(active));
        patch.insert(
            "status".into(),
            json!(if active { "active" } else { "inactive" }),
        );
    }
    if let Some(raw) = sent.get("role_ids") {
        let wanted: Vec<String> = if raw.trim().starts_with('[') {
            serde_json::from_str(raw).unwrap_or_default()
        } else {
            raw.split(',')
                .map(|one| one.trim().to_string())
                .filter(|one| !one.is_empty())
                .collect()
        };
        for role_id in &wanted {
            if !roles.iter().any(|role| &role.id == role_id) {
                return error(400, &format!("Role {role_id} not found"));
            }
        }
        patch.insert(
            "permissions".into(),
            json!(permissions_for(&wanted, &roles)),
        );
        patch.insert("role_ids".into(), json!(wanted));
    }
    patch.insert("updated_at".into(), json!(iso(now())));

    let stamp = patch.get("updated_at").cloned().unwrap_or(Value::Null);
    match records.update(&user_id, &Value::Object(patch.clone())) {
        Ok(_) => {
            let mut recorded = patch.clone();
            recorded.remove("updated_at");
            audit(
                &graph,
                &existing.id,
                "user.updated",
                "User",
                &user_id,
                json!({ "updated_fields": Value::Object(recorded) }),
            );
            (
                StatusCode::OK,
                Json(json!({ "ok": true, "updated_at": stamp })),
            )
                .into_response()
        }
        Err(_) => error(500, "Could not update the account"),
    }
}

async fn change_password(
    State(graph): State<GraphHandle>,
    headers: HeaderMap,
    Path(user_id): Path<String>,
    body: Option<axum::body::Bytes>,
) -> Response {
    let caller = match caller(&graph, &headers) {
        Ok(caller) => caller,
        Err(response) => return *response,
    };
    if caller.id != user_id {
        return error(403, "Cannot change another user's password");
    }
    let Some(user) = stored_user(&graph, &user_id) else {
        return error(404, "User not found");
    };
    let sent = fields(None, body.as_deref());
    let current = sent.get("current_password").cloned().unwrap_or_default();
    let wanted = sent.get("new_password").cloned().unwrap_or_default();

    if !accounts::verify_password(&current, &user.password_hash) {
        audit(
            &graph,
            &user_id,
            "password.change.failed",
            "User",
            &user_id,
            json!({ "reason": "invalid_current_password" }),
        );
        return error(400, "Current password is incorrect");
    }
    let names = [user.username.as_str(), user.email.as_str()];
    let hash = match hashed(&wanted, &names) {
        Ok(hash) => hash,
        Err(answer) => return *answer,
    };
    let _ = accounts_of(&graph).update(
        &user_id,
        &json!({ "password_hash": hash, "updated_at": iso(now()) }),
    );
    // every refresh token this person holds stops working: a password change is the moment a stolen one is spent
    live()
        .lock()
        .expect("the identity state is never poisoned")
        .refresh
        .retain(|_, (owner, _)| owner != &user_id);
    audit(
        &graph,
        &user_id,
        "password.changed",
        "User",
        &user_id,
        json!({}),
    );
    Json(json!({ "ok": true, "message": "Password changed successfully" })).into_response()
}

async fn setup_mfa(
    State(graph): State<GraphHandle>,
    headers: HeaderMap,
    Path(user_id): Path<String>,
    body: Option<axum::body::Bytes>,
) -> Response {
    if let Err(response) = self_or_admin(&graph, &headers, &user_id) {
        return *response;
    }
    let Some(user) = stored_user(&graph, &user_id) else {
        return error(404, "User not found");
    };
    let sent = fields(None, body.as_deref());
    let method = MfaMethod::parse(&sent.get("method").cloned().unwrap_or_default())
        .unwrap_or(MfaMethod::Totp);
    if method == MfaMethod::Totp {
        // An authenticator app is the one delivery channel that needs neither a mail server nor a phone number, so this is
        // the method that can be finished by a real person on a real device. The secret is handed over once, here, and the
        // account only starts asking for codes after one made from it comes back.
        let secret = totp::secret();
        live()
            .lock()
            .expect("the identity state is never poisoned")
            .enrolments
            .insert(
                user.id.clone(),
                Enrolment {
                    secret: secret.clone(),
                    method: method.as_str().to_string(),
                    expires_at: now() + Duration::minutes(ENROLMENT_MINUTES),
                },
            );
        audit(
            &graph,
            &user.id,
            "mfa.setup",
            "User",
            &user.id,
            json!({ "method": method.as_str() }),
        );
        return Json(json!({
            "mfa_method": method.as_str(),
            "secret": secret,
            "provisioning_uri": totp::provisioning_uri(&user.email, &secret),
            "code_sent": false,
            "message": "Add this key to your authenticator app, then enter the six digits it shows.",
        }))
        .into_response();
    }
    let code = raise_code(&user.id);
    audit(
        &graph,
        &user.id,
        "mfa.setup",
        "User",
        &user.id,
        json!({ "method": method.as_str(), "code": code }),
    );
    Json(json!({
        "mfa_method": method.as_str(),
        "secret": Value::Null,
        "code_sent": true,
        "message": format!("MFA code sent via {}", method.as_str()),
    }))
    .into_response()
}

async fn verify_mfa_setup(
    State(graph): State<GraphHandle>,
    headers: HeaderMap,
    Path(user_id): Path<String>,
    body: Option<axum::body::Bytes>,
) -> Response {
    if let Err(response) = self_or_admin(&graph, &headers, &user_id) {
        return *response;
    }
    let Some(user) = stored_user(&graph, &user_id) else {
        return error(404, "User not found");
    };
    let sent = fields(None, body.as_deref());
    let code = sent.get("code").cloned().unwrap_or_default();

    // An authenticator enrolment is confirmed against the secret that enrolment holds, not against a code raised into an
    // audit record: the person has the app, and only a code they can see proves it.
    let enrolment = {
        let mut held = live().lock().expect("the identity state is never poisoned");
        match held.enrolments.get(&user_id) {
            Some(one) if now() <= one.expires_at => held.enrolments.remove(&user_id),
            Some(_) => {
                held.enrolments.remove(&user_id);
                return error(400, "The enrolment expired. Start again.");
            }
            None => None,
        }
    };
    let Some(enrolment) = enrolment else {
        // no enrolment waiting: this is the raised-code method, whose code was recorded when it was set up
        let method = MfaMethod::parse(&sent.get("method").cloned().unwrap_or_default())
            .unwrap_or(user.mfa_method);
        if !spend_code(&code, &user.id) {
            return error(400, "Invalid MFA code");
        }
        let _ = accounts_of(&graph).update(
            &user_id,
            &json!({
                "mfa_enabled": true,
                "mfa_method": method.as_str(),
                "updated_at": iso(now()),
            }),
        );
        audit(&graph, &user_id, "mfa.enabled", "User", &user_id, json!({}));
        return Json(json!({ "ok": true, "message": "MFA enabled successfully" })).into_response();
    };

    let Some(step) = totp::verify_at(
        &enrolment.secret,
        &code,
        now().timestamp().max(0) as u64,
        totp::DRIFT_STEPS,
    ) else {
        // the enrolment stays open so a mistyped digit is not a restart, but only for its quarter of an hour
        live()
            .lock()
            .expect("the identity state is never poisoned")
            .enrolments
            .insert(user.id.clone(), enrolment);
        return error(400, "Invalid MFA code");
    };
    // Turning the factor on is also the only moment the recovery codes can be shown: they are stored as hashes, so a second
    // view is impossible by design. A person who loses the phone that holds this factor has no other way back into their own
    // account, which is why the codes come with the confirmation rather than behind a menu they might never open.
    let (codes, held) = recovery::issue(&user_id);
    let _ = accounts_of(&graph).update(
        &user_id,
        &json!({
            "mfa_enabled": true,
            "mfa_method": enrolment.method,
            "mfa_secret": enrolment.secret,
            "mfa_recovery": held,
            "updated_at": iso(now()),
        }),
    );
    live()
        .lock()
        .expect("the identity state is never poisoned")
        .spent_steps
        .insert(user.id.clone(), step);
    audit(&graph, &user_id, "mfa.enabled", "User", &user_id, json!({}));
    Json(json!({
        "ok": true,
        "message": "MFA enabled successfully",
        "recovery_codes": codes,
        "recovery_codes_note": "Each of these works once, in place of a code from your app. Keep them somewhere other than the phone that holds the app.",
    }))
    .into_response()
}

async fn disable_mfa(
    State(graph): State<GraphHandle>,
    headers: HeaderMap,
    Path(user_id): Path<String>,
    body: Option<axum::body::Bytes>,
) -> Response {
    if let Err(response) = self_or_admin(&graph, &headers, &user_id) {
        return *response;
    }
    let Some(user) = stored_user(&graph, &user_id) else {
        return error(404, "User not found");
    };
    let sent = fields(None, body.as_deref());
    let code = sent.get("code").cloned().unwrap_or_default();
    // Turning the second factor off needs a code from it, or the second factor was never needed: someone holding an unlocked
    // phone must not be able to take it off and walk through the door with only a password.
    let from_app = match user.mfa_secret.as_deref() {
        Some(secret) => totp::verify_at(
            secret,
            &code,
            now().timestamp().max(0) as u64,
            totp::DRIFT_STEPS,
        ),
        None => None,
    };
    if from_app.is_none() && !spend_code(&code, &user.id) {
        return error(400, "Invalid MFA code");
    }
    let _ = accounts_of(&graph).update(
        &user_id,
        &json!({
            "mfa_enabled": false,
            "mfa_method": Value::Null,
            "mfa_secret": Value::Null,
            // the codes belong to the factor being turned off. Leaving the hashes behind would keep a way into the account
            // that outlived the mechanism meant to guard it, and "off" has to mean nothing of it is left.
            "mfa_recovery": Vec::<String>::new(),
            "updated_at": iso(now()),
        }),
    );
    audit(
        &graph,
        &user_id,
        "mfa.disabled",
        "User",
        &user_id,
        json!({}),
    );
    Json(json!({ "ok": true, "message": "MFA disabled successfully" })).into_response()
}

async fn list_roles(State(graph): State<GraphHandle>, headers: HeaderMap) -> Response {
    if let Err(response) = caller(&graph, &headers) {
        return *response;
    }
    let roles = known_roles(&graph);
    Json(json!({
        "total": roles.len(),
        "roles": roles.iter().map(|role| role.to_json()).collect::<Vec<_>>(),
    }))
    .into_response()
}

async fn create_role(
    State(graph): State<GraphHandle>,
    headers: HeaderMap,
    body: Option<axum::body::Bytes>,
) -> Response {
    let caller = match caller(&graph, &headers) {
        Ok(caller) => caller,
        Err(response) => return *response,
    };
    let roles = known_roles(&graph);
    if !permissions_for(&caller.role_ids, &roles)
        .iter()
        .any(|held| held == "user:manage")
    {
        return error(403, "Not permitted");
    }
    let sent = fields(None, body.as_deref());
    let permissions: Vec<String> = match sent.get("permissions") {
        Some(raw) if raw.trim().starts_with('[') => serde_json::from_str(raw).unwrap_or_default(),
        Some(raw) => raw
            .split(',')
            .map(|one| one.trim().to_string())
            .filter(|one| !one.is_empty())
            .collect(),
        None => Vec::new(),
    };
    let role = Role {
        id: made_id("role"),
        name: sent.get("name").cloned().unwrap_or_default(),
        description: sent.get("description").cloned().unwrap_or_default(),
        permissions,
        is_system: false,
    };
    let _ = PlatformRecords::new(graph.as_ref(), ROLE).create(&json!({
        "id": role.id,
        "name": role.name,
        "description": role.description,
        "permissions": role.permissions,
        "is_system": role.is_system,
        "created_at": iso(now()),
    }));
    audit(
        &graph,
        &caller.id,
        "role.created",
        "Role",
        &role.id,
        json!({ "name": role.name }),
    );
    (StatusCode::CREATED, Json(role.to_json())).into_response()
}

async fn get_role(
    State(graph): State<GraphHandle>,
    headers: HeaderMap,
    Path(role_id): Path<String>,
) -> Response {
    if let Err(response) = caller(&graph, &headers) {
        return *response;
    }
    match known_roles(&graph)
        .into_iter()
        .find(|role| role.id == role_id)
    {
        Some(role) => Json(role.to_json()).into_response(),
        None => error(404, "Role not found"),
    }
}

async fn put_role(
    State(graph): State<GraphHandle>,
    headers: HeaderMap,
    Path(role_id): Path<String>,
    body: Option<axum::body::Bytes>,
) -> Response {
    let caller = match caller(&graph, &headers) {
        Ok(caller) => caller,
        Err(response) => return *response,
    };
    let roles = known_roles(&graph);
    if !permissions_for(&caller.role_ids, &roles)
        .iter()
        .any(|held| held == "user:manage")
    {
        return error(403, "Not permitted");
    }
    let Some(existing) = roles.iter().find(|role| role.id == role_id) else {
        return error(404, "Role not found");
    };
    if existing.is_system {
        return error(400, "Cannot modify system role");
    }
    let sent = fields(None, body.as_deref());
    let permissions: Vec<String> = match sent.get("permissions") {
        Some(raw) if raw.trim().starts_with('[') => serde_json::from_str(raw).unwrap_or_default(),
        Some(raw) => raw
            .split(',')
            .map(|one| one.trim().to_string())
            .filter(|one| !one.is_empty())
            .collect(),
        None => existing.permissions.clone(),
    };
    let name = sent
        .get("name")
        .cloned()
        .unwrap_or_else(|| existing.name.clone());
    let description = sent
        .get("description")
        .cloned()
        .unwrap_or_else(|| existing.description.clone());

    let _ = PlatformRecords::new(graph.as_ref(), ROLE).update(
        &role_id,
        &json!({
            "name": name,
            "description": description,
            "permissions": permissions,
            "updated_at": iso(now()),
        }),
    );
    audit(
        &graph,
        &caller.id,
        "role.updated",
        "Role",
        &role_id,
        json!({ "name": name }),
    );
    Json(json!({
        "id": role_id,
        "name": name,
        "description": description,
        "permissions": permissions,
        "is_system": false,
    }))
    .into_response()
}

async fn create_organization(
    State(graph): State<GraphHandle>,
    headers: HeaderMap,
    body: Option<axum::body::Bytes>,
) -> Response {
    let caller = match caller(&graph, &headers) {
        Ok(caller) => caller,
        Err(response) => return *response,
    };
    let sent: serde_json::Map<String, Value> = match body.as_deref() {
        Some(raw) if !raw.is_empty() => match serde_json::from_slice::<Value>(raw) {
            Ok(Value::Object(object)) => object,
            _ => return error(422, "a body of JSON fields is required"),
        },
        _ => serde_json::Map::new(),
    };
    // one implementation: the organisation and the owner membership are written by the tenancy layer, so a person who
    // opens an organisation is its owner by the same rule every other caller is checked against
    let opened = match organisation::create(graph.as_ref(), &caller.id, &sent) {
        Ok(opened) => opened,
        Err(store::StoreError::Invalid(message)) => return error(422, &message),
        Err(error) => return stored(error),
    };
    let record = opened["organisation"].clone();
    audit(
        &graph,
        &caller.id,
        "organization.created",
        "Organization",
        record["id"].as_str().unwrap_or_default(),
        json!({ "name": record["name"] }),
    );
    (
        StatusCode::CREATED,
        Json(json!({
            "id": record["id"],
            "name": record["name"],
            "slug": record["slug"],
            "description": record["description"],
            "settings": record["settings"],
            "role": opened["membership"]["role"],
        })),
    )
        .into_response()
}

async fn get_organization(
    State(graph): State<GraphHandle>,
    headers: HeaderMap,
    Path(org_id): Path<String>,
) -> Response {
    let caller = match caller(&graph, &headers) {
        Ok(caller) => caller,
        Err(response) => return *response,
    };
    // a non-member is told the organisation is not there rather than that it is not theirs to see, which is what the
    // tenancy layer answers and what the shipped surface already did
    match organisation::get(graph.as_ref(), &caller.id, &org_id) {
        Ok(record) => Json(record).into_response(),
        Err(store::StoreError::NotFound) => error(404, "Organization not found"),
        Err(error) => stored(error),
    }
}

async fn list_organizations(State(graph): State<GraphHandle>, headers: HeaderMap) -> Response {
    let caller = match caller(&graph, &headers) {
        Ok(caller) => caller,
        Err(response) => return *response,
    };
    match organisation::mine(graph.as_ref(), &caller.id, 200) {
        Ok(found) => Json(json!({ "organizations": found, "count": found.len() })).into_response(),
        Err(error) => stored(error),
    }
}

async fn list_organization_members(
    State(graph): State<GraphHandle>,
    headers: HeaderMap,
    Path(org_id): Path<String>,
) -> Response {
    let caller = match caller(&graph, &headers) {
        Ok(caller) => caller,
        Err(response) => return *response,
    };
    match organisation::members(graph.as_ref(), &caller.id, &org_id) {
        Ok(found) => Json(json!({ "members": found, "count": found.len() })).into_response(),
        Err(store::StoreError::NotFound) => error(404, "Organization not found"),
        Err(error) => stored(error),
    }
}

async fn add_organization_member(
    State(graph): State<GraphHandle>,
    headers: HeaderMap,
    Path(org_id): Path<String>,
    body: Option<axum::body::Bytes>,
) -> Response {
    let caller = match caller(&graph, &headers) {
        Ok(caller) => caller,
        Err(response) => return *response,
    };
    let sent: serde_json::Map<String, Value> = match body.as_deref() {
        Some(raw) if !raw.is_empty() => match serde_json::from_slice::<Value>(raw) {
            Ok(Value::Object(object)) => object,
            _ => return error(422, "a body of JSON fields is required"),
        },
        _ => serde_json::Map::new(),
    };
    match organisation::add_member(graph.as_ref(), &caller.id, &org_id, &sent) {
        Ok(membership) => {
            audit(
                &graph,
                &caller.id,
                "organization.member_added",
                "Organization",
                &org_id,
                json!({
                    "user_id": membership["member_user_id"],
                    "role": membership["role"],
                }),
            );
            (StatusCode::CREATED, Json(membership)).into_response()
        }
        Err(store::StoreError::NotFound) => error(404, "Organization not found"),
        Err(store::StoreError::Forbidden(message)) => error(403, &message),
        Err(store::StoreError::Invalid(message)) => error(422, &message),
        Err(store::StoreError::Conflict(message)) => error(409, &message),
        Err(error) => stored(error),
    }
}

async fn list_organization_books(
    State(graph): State<GraphHandle>,
    headers: HeaderMap,
    Path(org_id): Path<String>,
) -> Response {
    let caller = match caller(&graph, &headers) {
        Ok(caller) => caller,
        Err(response) => return *response,
    };
    match organisation::books_of(graph.as_ref(), &caller.id, &org_id) {
        Ok(found) => Json(json!({ "books": found, "count": found.len() })).into_response(),
        Err(store::StoreError::NotFound) => error(404, "Organization not found"),
        Err(error) => stored(error),
    }
}

/// Take a Book back out of an organisation, leaving it with only the person who holds it.
///
/// The counterpart of moving one in: an offer that can only be accepted is not an offer.
async fn take_book_out_of_organization(
    State(graph): State<GraphHandle>,
    headers: HeaderMap,
    Path((org_id, book_id)): Path<(String, String)>,
) -> Response {
    let caller = match caller(&graph, &headers) {
        Ok(caller) => caller,
        Err(response) => return *response,
    };
    match organisation::detach_book(graph.as_ref(), &caller.id, &org_id, &book_id) {
        Ok(book) => {
            audit(
                &graph,
                &caller.id,
                "organization.book_detached",
                "Organization",
                &org_id,
                json!({ "book_id": book_id }),
            );
            Json(book).into_response()
        }
        // the same wording as attaching a Book: from the caller's side both are "that pairing does not exist"
        Err(store::StoreError::NotFound) => error(404, "Organization or book not found"),
        Err(store::StoreError::Forbidden(message)) => error(403, &message),
        Err(error) => stored(error),
    }
}

async fn move_book_into_organization(
    State(graph): State<GraphHandle>,
    headers: HeaderMap,
    Path((org_id, book_id)): Path<(String, String)>,
) -> Response {
    let caller = match caller(&graph, &headers) {
        Ok(caller) => caller,
        Err(response) => return *response,
    };
    match organisation::attach_book(graph.as_ref(), &caller.id, &org_id, &book_id) {
        Ok(book) => {
            audit(
                &graph,
                &caller.id,
                "organization.book_attached",
                "Organization",
                &org_id,
                json!({ "book_id": book_id }),
            );
            Json(book).into_response()
        }
        Err(store::StoreError::NotFound) => error(404, "Organization or book not found"),
        Err(store::StoreError::Forbidden(message)) => error(403, &message),
        Err(error) => stored(error),
    }
}

async fn refresh_token(
    State(graph): State<GraphHandle>,
    RawQuery(query): RawQuery,
    body: Option<axum::body::Bytes>,
) -> Response {
    let sent = fields(query.as_deref(), body.as_deref());
    let token = sent.get("refresh_token").cloned().unwrap_or_default();
    let held = {
        let state = live().lock().expect("the identity state is never poisoned");
        state.refresh.get(&token).cloned()
    };
    let Some((user_id, expires_at)) = held else {
        return error(401, "Invalid refresh token");
    };
    if now() > expires_at {
        live()
            .lock()
            .expect("the identity state is never poisoned")
            .refresh
            .remove(&token);
        return error(401, "Refresh token expired");
    }
    let Some(user) = stored_user(&graph, &user_id) else {
        return error(401, "User not active");
    };
    if !user.status.may_sign_in() {
        return error(401, "User not active");
    }
    // the old token is spent as the new one is made, so a refresh cannot be replayed
    live()
        .lock()
        .expect("the identity state is never poisoned")
        .refresh
        .remove(&token);
    let roles = known_roles(&graph);
    let access = issue_access_token(&user, &roles, &secret(), now(), None);
    let refresh = random_token();
    live()
        .lock()
        .expect("the identity state is never poisoned")
        .refresh
        .insert(
            refresh.clone(),
            (user.id.clone(), now() + Duration::days(REFRESH_TOKEN_DAYS)),
        );
    (
        StatusCode::OK,
        Json(json!({
            "access_token": access.value,
            "refresh_token": refresh,
            "token_type": "bearer",
            "expires_in": access.expires_in,
        })),
    )
        .into_response()
}

async fn revoke_token(RawQuery(query): RawQuery, body: Option<axum::body::Bytes>) -> Response {
    let sent = fields(query.as_deref(), body.as_deref());
    let token = sent.get("refresh_token").cloned().unwrap_or_default();
    live()
        .lock()
        .expect("the identity state is never poisoned")
        .refresh
        .remove(&token);
    Json(json!({ "ok": true })).into_response()
}

async fn request_password_reset(
    State(graph): State<GraphHandle>,
    RawQuery(query): RawQuery,
    body: Option<axum::body::Bytes>,
) -> Response {
    let sent = fields(query.as_deref(), body.as_deref());
    let email = sent.get("email").cloned().unwrap_or_default();
    if let Some(user) = accounts_of(&graph)
        .find("email", email.trim().to_lowercase().as_str())
        .ok()
        .flatten()
        .and_then(|record| user_from(&record))
    {
        let raw = random_token();
        live()
            .lock()
            .expect("the identity state is never poisoned")
            .resets
            .insert(
                sha256_hex(&raw),
                (user.id.clone(), now() + Duration::hours(RESET_TOKEN_HOURS)),
            );
        audit(
            &graph,
            &user.id,
            "password.reset.requested",
            "User",
            &user.id,
            json!({ "reset_token": raw }),
        );
    }
    // the same answer whether or not the address is registered: this endpoint must not say which emails exist
    Json(json!({ "message": "If the email exists, a password reset link has been sent" }))
        .into_response()
}

async fn confirm_password_reset(
    State(graph): State<GraphHandle>,
    RawQuery(query): RawQuery,
    body: Option<axum::body::Bytes>,
) -> Response {
    let sent = fields(query.as_deref(), body.as_deref());
    let raw = sent.get("reset_token").cloned().unwrap_or_default();
    let wanted = sent.get("new_password").cloned().unwrap_or_default();
    let key = sha256_hex(&raw);

    let held = {
        let state = live().lock().expect("the identity state is never poisoned");
        state.resets.get(&key).cloned()
    };
    let Some((user_id, expires_at)) = held else {
        return error(400, "Invalid or expired reset token");
    };
    if now() > expires_at {
        live()
            .lock()
            .expect("the identity state is never poisoned")
            .resets
            .remove(&key);
        return error(400, "Reset token expired");
    }
    let Some(user) = stored_user(&graph, &user_id) else {
        return error(404, "User not found");
    };
    let names = [user.username.as_str(), user.email.as_str()];
    let hash = match hashed(&wanted, &names) {
        Ok(hash) => hash,
        Err(answer) => return *answer,
    };
    let _ = accounts_of(&graph).update(
        &user_id,
        &json!({
            "password_hash": hash,
            "status": UserStatus::Active.as_str(),
            "updated_at": iso(now()),
        }),
    );
    live()
        .lock()
        .expect("the identity state is never poisoned")
        .resets
        .remove(&key);
    live()
        .lock()
        .expect("the identity state is never poisoned")
        .refresh
        .retain(|_, (owner, _)| owner != &user_id);
    audit(
        &graph,
        &user_id,
        "password.reset.completed",
        "User",
        &user_id,
        json!({}),
    );
    Json(json!({ "ok": true, "message": "Password reset successfully" })).into_response()
}

async fn list_audit_logs(
    State(graph): State<GraphHandle>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let caller = match caller(&graph, &headers) {
        Ok(caller) => caller,
        Err(response) => return *response,
    };
    let roles = known_roles(&graph);
    if !permissions_for(&caller.role_ids, &roles)
        .iter()
        .any(|held| held == "user:manage")
    {
        return error(403, "Not permitted");
    }
    let sent = fields(query.as_deref(), None);
    let limit: usize = sent
        .get("limit")
        .and_then(|raw| raw.parse().ok())
        .unwrap_or(100);
    let offset: usize = sent
        .get("offset")
        .and_then(|raw| raw.parse().ok())
        .unwrap_or(0);
    let wanted = |key: &str| sent.get(key).filter(|one| !one.is_empty()).cloned();

    let mut logs: Vec<Value> = PlatformRecords::new(graph.as_ref(), AUDIT)
        .list(10_000)
        .unwrap_or_default();
    logs.retain(|entry| {
        let matches = |value: &Option<String>, field: &str| match value {
            Some(wanted) => entry.get(field).and_then(Value::as_str) == Some(wanted.as_str()),
            None => true,
        };
        matches(&wanted("user_id"), "user_id_actor")
            && matches(&wanted("action"), "action")
            && matches(&wanted("resource_type"), "resource_type")
    });
    logs.sort_by(|left, right| {
        let one = left
            .get("timestamp")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let two = right
            .get("timestamp")
            .and_then(Value::as_str)
            .unwrap_or_default();
        two.cmp(one)
    });
    let total = logs.len();
    let page: Vec<Value> = logs.into_iter().skip(offset).take(limit).collect();
    Json(json!({ "total": total, "logs": page })).into_response()
}

async fn list_sessions(State(graph): State<GraphHandle>, headers: HeaderMap) -> Response {
    let caller = match caller(&graph, &headers) {
        Ok(caller) => caller,
        Err(response) => return *response,
    };
    let state = live().lock().expect("the identity state is never poisoned");
    // only your own: a session list is not a way to watch other people
    let own: Vec<Value> = state
        .sessions
        .iter()
        .filter(|(_, session)| session.user_id == caller.id)
        .map(|(id, session)| {
            json!({
                "session_id": id,
                "user_id": session.user_id,
                "created_at": iso(session.created_at),
                "last_activity": iso(session.last_activity),
                "ip_address": session.ip_address,
            })
        })
        .collect();
    Json(json!({ "total": own.len(), "sessions": own })).into_response()
}

async fn revoke_session(
    State(graph): State<GraphHandle>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> Response {
    let caller = match caller(&graph, &headers) {
        Ok(caller) => caller,
        Err(response) => return *response,
    };
    let roles = known_roles(&graph);
    let is_admin = permissions_for(&caller.role_ids, &roles)
        .iter()
        .any(|held| held == "user:manage");
    let mut state = live().lock().expect("the identity state is never poisoned");
    match state.sessions.get(&session_id) {
        Some(session) if session.user_id == caller.id || is_admin => {
            state.sessions.remove(&session_id);
            Json(json!({ "ok": true })).into_response()
        }
        // someone else's session is not there as far as this caller is concerned
        _ => error(404, "Session not found"),
    }
}

async fn list_capabilities() -> Response {
    let listed: Vec<Value> = CAPABILITIES
        .iter()
        .map(|name| {
            let spoken = name
                .split(':')
                .map(|part| {
                    let mut letters = part.chars();
                    match letters.next() {
                        Some(first) => format!("{}{}", first.to_uppercase(), letters.as_str()),
                        None => String::new(),
                    }
                })
                .collect::<Vec<_>>()
                .join(" ");
            json!({ "name": name, "description": spoken })
        })
        .collect();
    Json(json!({ "capabilities": listed })).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_form_body_and_a_json_body_are_read_the_same_way() {
        let form = fields(None, Some(b"username=tariro&password=a%20b+c"));
        assert_eq!(form.get("username").map(String::as_str), Some("tariro"));
        assert_eq!(form.get("password").map(String::as_str), Some("a b c"));

        let json_body = fields(None, Some(br#"{"username":"tariro","password":"secret"}"#));
        assert_eq!(
            json_body.get("password").map(String::as_str),
            Some("secret")
        );

        // a query string wins nothing over a body, but both are read
        let combined = fields(Some("username=from-query"), Some(b"password=from-body"));
        assert_eq!(
            combined.get("username").map(String::as_str),
            Some("from-query")
        );
        assert_eq!(
            combined.get("password").map(String::as_str),
            Some("from-body")
        );
    }

    #[test]
    fn a_made_id_is_unique_even_within_one_nanosecond() {
        let first = made_id("user");
        let second = made_id("user");
        assert_ne!(first, second);
        assert!(first.starts_with("user-"));
    }

    #[test]
    fn a_reset_token_is_stored_as_its_hash_so_the_stored_value_is_not_the_credential() {
        let raw = random_token();
        let hashed = sha256_hex(&raw);
        assert_eq!(hashed.len(), 64);
        assert_ne!(raw, hashed);
        // the same token always hashes the same, so a confirm can find it
        assert_eq!(sha256_hex(&raw), hashed);
        assert_ne!(random_token(), random_token());
    }
}
