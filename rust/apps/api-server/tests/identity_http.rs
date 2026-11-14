//! identity-service answers at the addresses it published, and the door behaves as it was shipped.
//!
//! This is the one service the gateway leaves open: every other call carries a token minted here. So these tests walk
//! the paths a client already calls - register, sign in, read yourself, change a password, turn a second factor on,
//! refresh a token, log out - and pin the behaviour that matters more than the plumbing: the door never says whether
//! an account exists, an inactive account is refused even while holding a valid token, and a token that has been
//! revoked or has expired stops working.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use serde_json::{json, Value};
use store::fake::MemoryGraph;
use tower::ServiceExt;

const PREFIX: &str = "/identity";

async fn call(
    graph: Arc<MemoryGraph>,
    method: Method,
    path: &str,
    body: Option<Value>,
    token: Option<&str>,
) -> (StatusCode, Value) {
    let router = vimbai_api::records::router(graph);
    let mut builder = Request::builder()
        .method(method)
        .uri(format!("{PREFIX}{path}"))
        .header("content-type", "application/json");
    if let Some(token) = token {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }
    let body = match body {
        Some(value) => Body::from(value.to_string()),
        None => Body::empty(),
    };
    let response = router.oneshot(builder.body(body).unwrap()).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 200_000)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn registered(graph: Arc<MemoryGraph>, username: &str, password: &str) -> Value {
    let (status, body) = call(
        graph,
        Method::POST,
        "/users/register",
        Some(json!({
            "email": format!("{username}@example.com"),
            "username": username,
            "first_name": "Tariro",
            "last_name": "Moyo",
            "role_ids": ["accountant"],
            "password": password,
        })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    body
}

async fn signed_in(graph: Arc<MemoryGraph>, username: &str, password: &str) -> Value {
    let (status, body) = call(
        graph,
        Method::POST,
        "/users/login",
        Some(json!({ "username": username, "password": password })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body
}

async fn registered_role(
    graph: Arc<MemoryGraph>,
    username: &str,
    password: &str,
    role: &str,
) -> Value {
    let (status, body) = call(
        graph,
        Method::POST,
        "/users/register",
        Some(json!({
            "email": format!("{username}@example.com"),
            "username": username,
            "first_name": "Tariro",
            "last_name": "Moyo",
            "role_ids": [role],
            "password": password,
        })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    body
}

/// The six digits the last named action raised, read from the audit trail by an administrator. This deployment has no
/// email or SMS channel, so the raise is recorded rather than sent - which is also how an operator helps someone who
/// is locked out of their second factor.
async fn latest_code(graph: Arc<MemoryGraph>, token: &str, action: &str) -> String {
    let (status, body) = call(
        graph,
        Method::GET,
        &format!("/audit-logs?action={action}&limit=1"),
        None,
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["logs"][0]["details"]["code"]
        .as_str()
        .expect("the raise was recorded with its code")
        .to_string()
}

#[tokio::test]
async fn the_door_opens_on_a_health_check_and_a_new_account_can_sign_in() {
    let graph = Arc::new(MemoryGraph::new());
    let (status, body) = call(graph.clone(), Method::GET, "/", None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["service"], json!("identity-service"));
    assert_eq!(body["total_users"], json!(0));

    let made = registered(graph.clone(), "tariro", "correct horse battery").await;
    // the shipped answer carried only these three fields, and clients were written against it
    assert_eq!(made["username"], json!("tariro"));
    assert_eq!(made["email"], json!("tariro@example.com"));
    assert!(made["id"].as_str().is_some_and(|id| !id.is_empty()));

    let (status, body) = call(graph.clone(), Method::GET, "/", None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body["total_users"],
        json!(1),
        "the account is a record, not a hope"
    );

    let session = signed_in(graph, "tariro", "correct horse battery").await;
    assert!(session["access_token"]
        .as_str()
        .is_some_and(|one| !one.is_empty()));
    assert!(session["refresh_token"]
        .as_str()
        .is_some_and(|one| !one.is_empty()));
    assert_eq!(session["token_type"], json!("bearer"));
    assert_eq!(session["expires_in"], json!(3600));
}

#[tokio::test]
async fn one_answer_for_a_wrong_password_and_an_account_that_is_not_there() {
    let graph = Arc::new(MemoryGraph::new());
    registered(graph.clone(), "tariro", "correct horse battery").await;

    let (wrong_status, wrong) = call(
        graph.clone(),
        Method::POST,
        "/users/login",
        Some(json!({ "username": "tariro", "password": "not the password" })),
        None,
    )
    .await;
    let (unknown_status, unknown) = call(
        graph.clone(),
        Method::POST,
        "/users/login",
        Some(json!({ "username": "nobody", "password": "not the password" })),
        None,
    )
    .await;
    assert_eq!(wrong_status, StatusCode::UNAUTHORIZED);
    assert_eq!(unknown_status, StatusCode::UNAUTHORIZED);
    assert_eq!(wrong, unknown, "the door must not say which accounts exist");
    assert_eq!(wrong["detail"], json!("Invalid credentials"));
}

#[tokio::test]
async fn the_same_password_works_written_as_an_email_and_a_duplicate_cannot_be_registered() {
    let graph = Arc::new(MemoryGraph::new());
    registered(graph.clone(), "tariro", "correct horse battery").await;

    let session = signed_in(graph.clone(), "TARIRO@EXAMPLE.COM", "correct horse battery").await;
    assert!(session["access_token"].as_str().is_some());

    // and the same two collisions the shipped service refused, refused again
    let (status, body) = call(
        graph.clone(),
        Method::POST,
        "/users/register",
        Some(json!({
            "email": "someone@example.com",
            "username": "tariro",
            "first_name": "Other",
            "last_name": "Person",
            "role_ids": ["viewer"],
            "password": "another password",
        })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["detail"], json!("Username already taken"));

    let (status, body) = call(
        graph.clone(),
        Method::POST,
        "/users/register",
        Some(json!({
            "email": "tariro@example.com",
            "username": "someone",
            "first_name": "Other",
            "last_name": "Person",
            "role_ids": ["viewer"],
            "password": "another password",
        })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["detail"], json!("Email already registered"));

    let (status, body) = call(
        graph,
        Method::POST,
        "/users/register",
        Some(json!({
            "email": "new@example.com",
            "username": "new",
            "first_name": "New",
            "last_name": "Person",
            "role_ids": ["not-a-role"],
            "password": "another password",
        })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["detail"], json!("Role not-a-role not found"));
}

#[tokio::test]
async fn the_person_a_token_names_is_the_person_it_answers_for() {
    let graph = Arc::new(MemoryGraph::new());
    registered(graph.clone(), "tariro", "correct horse battery").await;
    let session = signed_in(graph.clone(), "tariro", "correct horse battery").await;
    let token = session["access_token"].as_str().unwrap().to_string();

    let (status, body) = call(graph.clone(), Method::GET, "/users/me", None, Some(&token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["username"], json!("tariro"));
    // the permissions come from the person's roles, resolved now rather than read out of the token
    assert!(
        body["permissions"]
            .as_array()
            .is_some_and(|held| held.iter().any(|one| one == "journal:post")),
        "{body}"
    );

    // no token, and rubbish, both refuse
    let (status, _) = call(graph.clone(), Method::GET, "/users/me", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = call(
        graph.clone(),
        Method::GET,
        "/users/me",
        None,
        Some("not-a-token"),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // a refresh token is not a session: it cannot be spent as one
    let refresh = session["refresh_token"].as_str().unwrap().to_string();
    let (status, _) = call(
        graph.clone(),
        Method::GET,
        "/users/me",
        None,
        Some(&refresh),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_refresh_token_is_spent_when_used_and_stops_working_when_revoked() {
    let graph = Arc::new(MemoryGraph::new());
    registered(graph.clone(), "tariro", "correct horse battery").await;
    let session = signed_in(graph.clone(), "tariro", "correct horse battery").await;
    let first = session["refresh_token"].as_str().unwrap().to_string();

    let (status, refreshed) = call(
        graph.clone(),
        Method::POST,
        "/token/refresh",
        Some(json!({ "refresh_token": first })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let second = refreshed["refresh_token"].as_str().unwrap().to_string();
    assert_ne!(
        first, second,
        "a refresh makes a new token, it does not hand back the old one"
    );

    // the first is now spent: replaying it is refused
    let (status, _) = call(
        graph.clone(),
        Method::POST,
        "/token/refresh",
        Some(json!({ "refresh_token": first })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // logging out spends the one that is live
    let (status, body) = call(
        graph.clone(),
        Method::POST,
        "/token/revoke",
        Some(json!({ "refresh_token": second })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["ok"], json!(true));
    let (status, _) = call(
        graph,
        Method::POST,
        "/token/refresh",
        Some(json!({ "refresh_token": second })),
        None,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "a revoked token does not refresh"
    );
}

#[tokio::test]
async fn changing_a_password_takes_the_current_one_and_retires_every_refresh_token() {
    let graph = Arc::new(MemoryGraph::new());
    let made = registered(graph.clone(), "tariro", "correct horse battery").await;
    let id = made["id"].as_str().unwrap().to_string();
    let session = signed_in(graph.clone(), "tariro", "correct horse battery").await;
    let token = session["access_token"].as_str().unwrap().to_string();
    let refresh = session["refresh_token"].as_str().unwrap().to_string();

    // the wrong current password is refused, and the account still opens with the old one
    let (status, body) = call(
        graph.clone(),
        Method::POST,
        &format!("/users/{id}/change-password"),
        Some(json!({"current_password": "wrong", "new_password": "a new password"})),
        Some(&token),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["detail"], json!("Current password is incorrect"));

    let (status, _) = call(
        graph.clone(),
        Method::POST,
        &format!("/users/{id}/change-password"),
        Some(
            json!({"current_password": "correct horse battery", "new_password": "a new password"}),
        ),
        Some(&token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    signed_in(graph.clone(), "tariro", "a new password").await;
    let (status, _) = call(
        graph.clone(),
        Method::POST,
        "/users/login",
        Some(json!({ "username": "tariro", "password": "correct horse battery" })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "the old password is gone");

    // and the token held before the change stops refreshing: that is the point of changing it
    let (status, _) = call(
        graph,
        Method::POST,
        "/token/refresh",
        Some(json!({ "refresh_token": refresh })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn changing_someone_elses_password_is_refused() {
    let graph = Arc::new(MemoryGraph::new());
    registered(graph.clone(), "tariro", "correct horse battery").await;
    let other = registered(graph.clone(), "rudo", "another horse battery").await;
    let session = signed_in(graph.clone(), "tariro", "correct horse battery").await;
    let token = session["access_token"].as_str().unwrap().to_string();

    let (status, body) = call(
        graph,
        Method::POST,
        &format!("/users/{}/change-password", other["id"].as_str().unwrap()),
        Some(json!({"current_password": "another horse battery", "new_password": "taken over"})),
        Some(&token),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(
        body["detail"],
        json!("Cannot change another user's password")
    );
}

#[tokio::test]
async fn a_second_factor_is_required_before_a_session_is_handed_out() {
    let graph = Arc::new(MemoryGraph::new());
    // an administrator, because reading the audit trail is how this deployment hands a code over: there is no
    // channel to send one on, so the raise is recorded and an operator reads it
    registered_role(graph.clone(), "tariro", "correct horse battery", "admin").await;
    let session = signed_in(graph.clone(), "tariro", "correct horse battery").await;
    let token = session["access_token"].as_str().unwrap().to_string();
    let me = call(graph.clone(), Method::GET, "/users/me", None, Some(&token))
        .await
        .1;
    let id = me["id"].as_str().unwrap().to_string();

    // turn it on the shipped way: a code is raised, then the code turns it on
    let (status, body) = call(
        graph.clone(),
        Method::POST,
        &format!("/users/{id}/mfa/setup"),
        Some(json!({"method": "totp"})),
        Some(&token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["mfa_method"], json!("totp"));
    assert!(body["secret"]
        .as_str()
        .is_some_and(|secret| !secret.is_empty()));
    let raised = latest_code(graph.clone(), &token, "mfa.setup").await;

    let (status, body) = call(
        graph.clone(),
        Method::POST,
        &format!("/users/{id}/mfa/verify"),
        Some(json!({"code": raised, "method": "totp"})),
        Some(&token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["message"], json!("MFA enabled successfully"));

    // sign-in now stops at the challenge: a token that says one is owed, and no session
    let (status, body) = call(
        graph.clone(),
        Method::POST,
        "/users/login",
        Some(json!({ "username": "tariro", "password": "correct horse battery" })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["mfa_required"], json!(true));
    assert_eq!(body["method"], json!("totp"));
    assert!(
        body["access_token"].is_null(),
        "no session before the second factor"
    );
    let challenge = body["temp_token"].as_str().unwrap().to_string();

    // the challenge token is not a session of its own
    let (status, _) = call(
        graph.clone(),
        Method::GET,
        "/users/me",
        None,
        Some(&challenge),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // a code that was never raised is refused, and so is the access token used as if it were a challenge
    let (status, body) = call(
        graph.clone(),
        Method::POST,
        "/users/login/verify-mfa",
        Some(json!({"temp_token": challenge, "code": "000000"})),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["detail"], json!("Invalid MFA code"));
}

#[tokio::test]
async fn an_access_token_cannot_be_spent_as_a_challenge() {
    let graph = Arc::new(MemoryGraph::new());
    registered(graph.clone(), "tariro", "correct horse battery").await;
    let session = signed_in(graph.clone(), "tariro", "correct horse battery").await;
    let token = session["access_token"].as_str().unwrap().to_string();

    let (status, body) = call(
        graph,
        Method::POST,
        "/users/login/verify-mfa",
        Some(json!({"temp_token": token, "code": "123456"})),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["detail"], json!("Invalid temporary token"));
}

#[tokio::test]
async fn a_password_reset_says_the_same_thing_whether_or_not_the_address_is_known() {
    let graph = Arc::new(MemoryGraph::new());
    registered(graph.clone(), "tariro", "correct horse battery").await;

    let (known_status, known) = call(
        graph.clone(),
        Method::POST,
        "/password/reset",
        Some(json!({"email": "tariro@example.com"})),
        None,
    )
    .await;
    let (unknown_status, unknown) = call(
        graph.clone(),
        Method::POST,
        "/password/reset",
        Some(json!({"email": "nobody@example.com"})),
        None,
    )
    .await;
    assert_eq!(known_status, StatusCode::OK);
    assert_eq!(unknown_status, StatusCode::OK);
    assert_eq!(
        known, unknown,
        "this endpoint must not say which emails exist"
    );

    // a token that was never raised cannot be spent
    let (status, body) = call(
        graph,
        Method::POST,
        "/password/reset/confirm",
        Some(json!({"reset_token": "never-raised", "new_password": "a new password"})),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["detail"], json!("Invalid or expired reset token"));
}

#[tokio::test]
async fn the_roles_are_the_shipped_five_and_a_system_role_cannot_be_rewritten() {
    let graph = Arc::new(MemoryGraph::new());
    registered(graph.clone(), "tariro", "correct horse battery").await;
    let session = signed_in(graph.clone(), "tariro", "correct horse battery").await;
    let token = session["access_token"].as_str().unwrap().to_string();

    let (status, body) = call(graph.clone(), Method::GET, "/roles", None, Some(&token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["total"], json!(5));
    let ids: Vec<String> = body["roles"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|role| role["id"].as_str().map(str::to_owned))
        .collect();
    assert_eq!(
        ids,
        vec!["admin", "accountant", "finance_manager", "viewer", "user"]
    );
    // the administrator holds every capability
    let admin = body["roles"]
        .as_array()
        .unwrap()
        .iter()
        .find(|role| role["id"] == json!("admin"))
        .unwrap();
    assert_eq!(admin["permissions"].as_array().unwrap().len(), 22);

    // an accountant may not rewrite one, and an administrator may not rewrite a system one either
    let (status, _) = call(
        graph.clone(),
        Method::PUT,
        "/roles/viewer",
        Some(json!({"name": "Sneaky", "permissions": ["role:manage"]})),
        Some(&token),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn the_capability_reference_is_readable_without_a_token() {
    let graph = Arc::new(MemoryGraph::new());
    let (status, body) = call(graph, Method::GET, "/capabilities", None, None).await;
    assert_eq!(status, StatusCode::OK);
    let listed = body["capabilities"].as_array().unwrap();
    assert_eq!(listed.len(), 22);
    let first = &listed[0];
    assert_eq!(first["name"], json!("account:view"));
    assert_eq!(first["description"], json!("Account View"));
}

#[tokio::test]
async fn your_own_sessions_are_listed_and_a_stranger_cannot_revoke_one() {
    let graph = Arc::new(MemoryGraph::new());
    registered(graph.clone(), "tariro", "correct horse battery").await;
    registered(graph.clone(), "rudo", "another horse battery").await;
    let mine = signed_in(graph.clone(), "tariro", "correct horse battery").await;
    let token = mine["access_token"].as_str().unwrap().to_string();
    let theirs = signed_in(graph.clone(), "rudo", "another horse battery").await;
    let their_token = theirs["access_token"].as_str().unwrap().to_string();

    let (status, body) = call(graph.clone(), Method::GET, "/sessions", None, Some(&token)).await;
    assert_eq!(status, StatusCode::OK);
    // two sign-ins happened, one each, so each person sees exactly their own
    assert_eq!(body["total"], json!(1));
    let session_id = body["sessions"][0]["session_id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(body["sessions"][0]["user_id"], json!(mine_id(&body)));

    // the other person's session is not there as far as this caller is concerned
    let (status, body) = call(
        graph.clone(),
        Method::DELETE,
        "/sessions/not-a-session",
        None,
        Some(&token),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["detail"], json!("Session not found"));

    // your own is yours to end
    let (status, body) = call(
        graph.clone(),
        Method::DELETE,
        &format!("/sessions/{session_id}"),
        None,
        Some(&token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["ok"], json!(true));

    // and the other person is untouched
    let (status, body) = call(graph, Method::GET, "/sessions", None, Some(&their_token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["total"], json!(1));
}

/// The id the listed session names, so a session is shown to belong to the person who asked for it.
fn mine_id(listed: &Value) -> Value {
    listed["sessions"][0]["user_id"].clone()
}

#[tokio::test]
async fn an_account_that_is_no_longer_active_cannot_ride_a_token_it_already_holds() {
    let graph = Arc::new(MemoryGraph::new());
    let made = registered(graph.clone(), "tariro", "correct horse battery").await;
    let id = made["id"].as_str().unwrap().to_string();
    let session = signed_in(graph.clone(), "tariro", "correct horse battery").await;
    let token = session["access_token"].as_str().unwrap().to_string();

    // the person themselves switches the account off, the shipped way
    let (status, _) = call(
        graph.clone(),
        Method::PUT,
        &format!("/users/{id}"),
        Some(json!({"is_active": false})),
        Some(&token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // the token still verifies, but the account it names may not come in
    let (status, body) = call(graph.clone(), Method::GET, "/users/me", None, Some(&token)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["detail"], json!("Account is not active"));

    // and signing in says the same thing, distinct from a wrong password
    let (status, body) = call(
        graph,
        Method::POST,
        "/users/login",
        Some(json!({ "username": "tariro", "password": "correct horse battery" })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["detail"], json!("Account is not active"));
}

#[tokio::test]
async fn an_organisation_is_readable_by_its_members_and_invisible_to_everyone_else() {
    let graph = Arc::new(MemoryGraph::new());
    registered(graph.clone(), "tariro", "correct horse battery").await;
    let session = signed_in(graph.clone(), "tariro", "correct horse battery").await;
    let token = session["access_token"].as_str().unwrap().to_string();

    let (status, made) = call(
        graph.clone(),
        Method::POST,
        "/organizations",
        Some(json!({"name": "Moyo Trading", "description": "A book of books"})),
        Some(&token),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let org_id = made["id"].as_str().unwrap().to_string();
    assert_eq!(made["name"], json!("Moyo Trading"));

    // whoever opens an organisation owns it, so it is there for them - and it says so
    let (status, body) = call(
        graph.clone(),
        Method::GET,
        &format!("/organizations/{org_id}"),
        None,
        Some(&token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["role"], json!("owner"));

    // and it is not there for somebody who is not in it: a stranger is told the organisation is missing rather than
    // that it is not theirs to see, which would be a directory of other people's tenants
    registered(graph.clone(), "sam", "another horse battery").await;
    let stranger = signed_in(graph.clone(), "sam", "another horse battery").await;
    let stranger_token = stranger["access_token"].as_str().unwrap().to_string();
    let (status, body) = call(
        graph.clone(),
        Method::GET,
        &format!("/organizations/{org_id}"),
        None,
        Some(&stranger_token),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["detail"], json!("Organization not found"));
}
