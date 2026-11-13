//! admin-service over HTTP: the feature catalogue, one caller's view of it, and the trail of what was changed.

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use serde_json::{json, Value};
use std::sync::Arc;
use store::fake::MemoryGraph;
use tower::ServiceExt;

fn router() -> axum::Router {
    vimbai_api::records::router(Arc::new(MemoryGraph::new()))
}

async fn call(
    router: &axum::Router,
    method: Method,
    path: &str,
    user: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header("x-user-id", user)
        .header("x-book-id", "book-1");
    let request = match body {
        Some(body) => {
            builder = builder.header("content-type", "application/json");
            builder.body(Body::from(body.to_string())).unwrap()
        }
        None => builder.body(Body::empty()).unwrap(),
    };
    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1_000_000)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

fn state_of(listed: &Value, id: &str) -> String {
    listed
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .find(|feature| feature["id"] == json!(id))
        .map(|feature| feature["status"].as_str().unwrap_or_default().to_string())
        .unwrap_or_default()
}

#[tokio::test]
async fn a_feature_switched_off_is_off_for_the_caller_who_did_it_and_nobody_else() {
    let router = router();

    let (status, catalogue) = call(&router, Method::GET, "/features", "user-1", None).await;
    assert_eq!(status, StatusCode::OK, "{catalogue}");
    assert_eq!(state_of(&catalogue, "double_entry"), "enabled");
    assert_eq!(catalogue.as_array().map(Vec::len), Some(28));

    // the switch answers what it did
    let (status, switched) = call(
        &router,
        Method::POST,
        "/features/double_entry/disable",
        "user-1",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{switched}");
    assert_eq!(switched["status"], json!("disabled"));

    let (_, mine) = call(&router, Method::GET, "/features", "user-1", None).await;
    assert_eq!(state_of(&mine, "double_entry"), "disabled");
    // the other person's catalogue is untouched
    let (_, theirs) = call(&router, Method::GET, "/features", "user-2", None).await;
    assert_eq!(state_of(&theirs, "double_entry"), "enabled");

    // a feature the catalogue does not hold is missing
    let (status, _) = call(
        &router,
        Method::GET,
        "/features/teleportation",
        "user-1",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // a named change is laid over the catalogue, and a share outside a hundred is refused
    let (status, changed) = call(
        &router,
        Method::PUT,
        "/features/budgeting",
        "user-1",
        Some(json!({"status": "beta", "rollout_percentage": 40})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{changed}");
    assert_eq!(changed["status"], json!("beta"));
    assert_eq!(changed["rollout_percentage"], json!(40));
    let (status, _) = call(
        &router,
        Method::PUT,
        "/features/budgeting",
        "user-1",
        Some(json!({"rollout_percentage": 140})),
    )
    .await;
    assert_ne!(status, StatusCode::OK);

    // the categories answer at their own path, which the shipped route never reached
    let (status, categories) =
        call(&router, Method::GET, "/feature-categories", "user-1", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(categories.as_array().map(Vec::len), Some(11));

    // filtering answers only what was asked for
    let (_, accounting) = call(
        &router,
        Method::GET,
        "/features?category=accounting",
        "user-1",
        None,
    )
    .await;
    assert!(accounting
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .all(|feature| feature["category"] == json!("accounting")));
}

#[tokio::test]
async fn an_organisation_rollout_and_a_request_are_recorded_for_the_caller() {
    let router = router();

    // a dependency is satisfied only while its dependencies are switched on
    let (_, before) = call(
        &router,
        Method::GET,
        "/features/forecasting/dependencies",
        "user-1",
        None,
    )
    .await;
    assert_eq!(before["satisfied"], json!(true));
    call(
        &router,
        Method::POST,
        "/features/budgeting/disable",
        "user-1",
        None,
    )
    .await;
    let (_, after) = call(
        &router,
        Method::GET,
        "/features/forecasting/dependencies",
        "user-1",
        None,
    )
    .await;
    assert_eq!(after["satisfied"], json!(false));
    assert_eq!(after["missing_dependencies"], json!(["budgeting"]));

    // an organisation is configured per feature, and the shortcuts mean a hundred and zero
    let (status, enabled) = call(
        &router,
        Method::PUT,
        "/organizations/org-1/features/budgeting?enabled=true",
        "user-1",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{enabled}");
    assert_eq!(enabled["enabled"], json!(true));
    assert_eq!(enabled["rollout_percentage"], json!(100));
    let (_, off) = call(
        &router,
        Method::POST,
        "/organizations/org-1/features/budgeting/disable",
        "user-1",
        None,
    )
    .await;
    assert_eq!(off["enabled"], json!(false));
    assert_eq!(off["rollout_percentage"], json!(0));

    let (_, org) = call(
        &router,
        Method::GET,
        "/organizations/org-1/features",
        "user-1",
        None,
    )
    .await;
    let budgeting = org
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .find(|feature| feature["id"] == json!("budgeting"))
        .expect("the feature");
    assert_eq!(budgeting["org_enabled"], json!(false));

    // a rollout is scheduled, then cancelled by the FEATURE's id, as the shipped store looked it up
    let (status, scheduled) = call(
        &router,
        Method::POST,
        "/rollout-schedules",
        "user-1",
        Some(json!({"feature_id": "budgeting", "scheduled_date": "2026-12-01T00:00:00Z", "target_percentage": 50})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{scheduled}");
    assert_eq!(scheduled["status"], json!("scheduled"));
    let (_, pending) = call(&router, Method::GET, "/rollout-schedules", "user-1", None).await;
    assert_eq!(pending.as_array().map(Vec::len), Some(1));
    let (status, cancelled) = call(
        &router,
        Method::DELETE,
        "/rollout-schedules/budgeting",
        "user-1",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{cancelled}");
    assert_eq!(cancelled["status"], json!("cancelled"));

    // a request keeps its requester, is reviewed, and can be withdrawn
    let (status, filed) = call(
        &router,
        Method::POST,
        "/feature-requests",
        "user-1",
        Some(json!({"user_id": "user-9", "user_email": "nine@vimbai.com", "feature_name": "Payroll export",
                    "priority": "high"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{filed}");
    assert_eq!(filed["user_id"], json!("user-9"));
    let id = filed["id"].as_str().unwrap_or_default().to_string();
    let (_, listed) = call(&router, Method::GET, "/feature-requests", "user-1", None).await;
    assert_eq!(listed.as_array().map(Vec::len), Some(1));
    // another person sees none of it
    let (_, theirs) = call(&router, Method::GET, "/feature-requests", "user-2", None).await;
    assert_eq!(theirs.as_array().map(Vec::len), Some(0));

    let (status, reviewed) = call(
        &router,
        Method::PUT,
        &format!("/feature-requests/{id}/review"),
        "user-1",
        Some(
            json!({"status": "approved", "reviewed_by": "admin", "review_notes": "with packaging"}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{reviewed}");
    assert_eq!(reviewed["status"], json!("approved"));
    assert_eq!(reviewed["reviewed_by"], json!("admin"));

    let (status, _) = call(
        &router,
        Method::DELETE,
        &format!("/feature-requests/{id}"),
        "user-1",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // configuration is held per caller, and a key the catalogue does not hold cannot be written
    let (status, changed) = call(
        &router,
        Method::PUT,
        "/config/company_name",
        "user-1",
        Some(json!({"value": "Vimbai Ltd"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{changed}");
    assert_eq!(changed["value"], json!("Vimbai Ltd"));
    let (_, mine) = call(&router, Method::GET, "/config/company_name", "user-1", None).await;
    assert_eq!(mine["value"], json!("Vimbai Ltd"));
    let (_, theirs) = call(&router, Method::GET, "/config/company_name", "user-2", None).await;
    assert_eq!(theirs["value"], json!("Vimbai Corporation"));
    let (status, _) = call(
        &router,
        Method::PUT,
        "/config/secret_handshake",
        "user-1",
        Some(json!({"value": "x"})),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // the trail says what was done and by whom, and the counts agree with it
    let (_, trail) = call(&router, Method::GET, "/audit-logs", "user-1", None).await;
    let entries = trail.as_array().cloned().unwrap_or_default();
    assert!(entries
        .iter()
        .any(|entry| entry["action"] == json!("config_updated")));
    assert!(entries
        .iter()
        .any(|entry| entry["action"] == json!("rollout_scheduled")));
    let (_, other_trail) = call(&router, Method::GET, "/audit-logs", "user-2", None).await;
    assert_eq!(other_trail.as_array().map(Vec::len), Some(0));

    let (_, posted) = call(
        &router,
        Method::POST,
        "/audit-logs",
        "user-1",
        Some(json!({"user_id": "user-9", "user_email": "nine@vimbai.com", "action": "export_requested",
                    "resource_type": "ledger", "resource_id": "entry-1"})),
    )
    .await;
    assert_eq!(posted["user_id"], json!("user-9"));

    let (status, stats) = call(&router, Method::GET, "/dashboard/stats", "user-1", None).await;
    assert_eq!(status, StatusCode::OK, "{stats}");
    assert_eq!(stats["total_features"], json!(28));
    assert_eq!(stats["total_config_entries"], json!(8));
    assert_eq!(stats["rollout_schedules"]["total"], json!(1));
    // seven entries: the organisation configured twice, the rollout, the request filed and reviewed, the configuration
    // value, and the line posted by hand
    assert_eq!(stats["audit_logs_count"], json!(7));

    // and the health rows are the shipped ones
    let (status, health) = call(&router, Method::GET, "/services/health", "user-1", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(health.as_array().map(Vec::len), Some(9));
}
