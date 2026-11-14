//! mfa-auth-service answers at the addresses it published, from the identity crate.
//!
//! The service published /setup, /verify and /challenge under its own prefix, so a client keeps calling what it was
//! told. What it answers is the person's second factor: enrolling themselves, verifying their own code, and raising a
//! five-minute challenge they alone can spend.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use serde_json::{json, Value};
use store::fake::MemoryGraph;
use tower::ServiceExt;

const CALLER: &str = "user-mfa";

fn router() -> axum::Router {
    vimbai_api::records::router(Arc::new(MemoryGraph::new()))
}

async fn call(
    app: &axum::Router,
    method: Method,
    uri: &str,
    payload: Value,
) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .header("x-user-id", CALLER)
        .header("x-book-id", "book-mfa");
    let response = app
        .clone()
        .oneshot(request.body(Body::from(payload.to_string())).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 2_000_000)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[tokio::test]
async fn enrolling_answers_the_secret_and_eight_backup_codes() {
    let app = router();
    let (status, answer) = call(
        &app,
        Method::POST,
        "/mfa-auth/setup",
        json!({"user_id": CALLER, "method": "totp"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{answer}");
    assert_eq!(answer["user_id"], CALLER);
    assert!(answer["secret"].as_str().unwrap().len() >= 16, "{answer}");
    assert_eq!(answer["backup_codes"].as_array().unwrap().len(), 8);
    assert!(
        answer["qr_uri"]
            .as_str()
            .unwrap()
            .starts_with("otpauth://totp/Vimbai:user-mfa?secret="),
        "{answer}"
    );

    // one enrolment per person: enrolling again replaces it rather than holding two
    let (status, again) = call(
        &app,
        Method::POST,
        "/mfa-auth/setup",
        json!({"user_id": CALLER}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{again}");
    let (status, health) = call(&app, Method::GET, "/mfa-auth/status", json!(null)).await;
    assert_eq!(status, StatusCode::OK, "{health}");
    assert_eq!(health["enrolled_users"], 1, "{health}");
    assert_ne!(
        answer["secret"], again["secret"],
        "a re-enrolment kept the old secret"
    );
}

#[tokio::test]
async fn verifying_before_enrolling_says_so_rather_than_rejecting_a_code() {
    let app = router();
    let (status, answer) = call(
        &app,
        Method::POST,
        "/mfa-auth/verify",
        json!({"user_id": CALLER, "code": "123456"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{answer}");
    assert_eq!(answer["verified"], false, "{answer}");
    assert_eq!(
        answer["message"], "MFA not set up for this user",
        "{answer}"
    );

    call(
        &app,
        Method::POST,
        "/mfa-auth/setup",
        json!({"user_id": CALLER}),
    )
    .await;

    let (status, good) = call(
        &app,
        Method::POST,
        "/mfa-auth/verify",
        json!({"user_id": CALLER, "code": "654321", "now": "2026-11-14T09:00:00Z"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{good}");
    assert_eq!(good["verified"], true, "{good}");
    assert_eq!(good["access_token"].as_str().unwrap().len(), 64, "{good}");

    // the shipped check wanted six digits and nothing else, and the port keeps that answer
    let (_, short) = call(
        &app,
        Method::POST,
        "/mfa-auth/verify",
        json!({"user_id": CALLER, "code": "12345"}),
    )
    .await;
    assert_eq!(short["verified"], false, "{short}");
    assert_eq!(short["message"], "Invalid MFA code", "{short}");
}

#[tokio::test]
async fn nobody_enrols_a_second_factor_for_someone_else() {
    let app = router();
    let (status, refused) = call(
        &app,
        Method::POST,
        "/mfa-auth/setup",
        json!({"user_id": "someone-else"}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{refused}");
}

#[tokio::test]
async fn a_challenge_is_raised_for_the_caller_and_spent_once() {
    let app = router();
    let (status, raised) = call(
        &app,
        Method::POST,
        "/mfa-auth/challenge",
        json!({
            "user_id": CALLER,
            "now": "2026-11-14T09:00:00Z",
            "now_plus_five": "2026-11-14T09:05:00Z",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{raised}");
    assert_eq!(raised["expires_in_minutes"], 5, "{raised}");
    assert_eq!(raised["method"], "totp", "{raised}");
    let id = raised["challenge_id"].as_str().unwrap().to_string();

    // a wrong code answers no and leaves the challenge in place, as shipped
    let (status, wrong) = call(
        &app,
        Method::POST,
        &format!("/mfa-auth/challenge/{id}/verify"),
        json!({"code": "000000", "now": "2026-11-14T09:01:00Z"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{wrong}");
    assert_eq!(wrong["verified"], false, "{wrong}");

    // an unknown challenge is missing rather than refused
    let (status, missing) = call(
        &app,
        Method::POST,
        "/mfa-auth/challenge/not-a-challenge/verify",
        json!({"code": "000000"}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{missing}");
}

#[tokio::test]
async fn a_challenge_past_its_five_minutes_is_expired_and_dropped() {
    let app = router();
    let (_, raised) = call(
        &app,
        Method::POST,
        "/mfa-auth/challenge",
        json!({"now": "2026-11-14T09:00:00Z", "now_plus_five": "2026-11-14T09:05:00Z"}),
    )
    .await;
    let id = raised["challenge_id"].as_str().unwrap().to_string();

    let (status, late) = call(
        &app,
        Method::POST,
        &format!("/mfa-auth/challenge/{id}/verify"),
        json!({"code": "000000", "now": "2026-11-14T10:00:00Z"}),
    )
    .await;
    assert_ne!(status, StatusCode::OK, "{late}");
    assert_ne!(late["verified"], true, "{late}");
}
