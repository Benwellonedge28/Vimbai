#![forbid(unsafe_code)]
//! automation-engine-service over HTTP: a rule, its switch, and the runs it produced.

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
    uri: &str,
    payload: Value,
) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .header("x-user-id", "user-1")
        .header("x-book-id", "book-1")
        .body(Body::from(payload.to_string()))
        .unwrap();
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

#[tokio::test]
async fn a_rule_runs_its_steps_and_a_broken_dependency_stops_it() {
    let router = router();

    let (status, made) = call(
        &router,
        Method::POST,
        "/automation-rules",
        json!({"name": "Month end sweep", "company_id": "co-1", "trigger": "scheduled",
        "steps": [
            {"step_id": "s1", "step_name": "Post accruals", "action": "post_journal"},
            {"step_id": "s2", "step_name": "Reconcile", "action": "reconcile", "depends_on": ["s1"]}
        ]}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{made}");
    assert_eq!(made["enabled"], json!(true));
    assert_eq!(made["trigger"], json!("scheduled"));
    let id = made["id"].as_str().expect("an id").to_string();

    // a trigger nobody agreed on is a bad request
    let (status, _) = call(
        &router,
        Method::POST,
        "/automation-rules",
        json!({"name": "x", "trigger": "whenever"}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // the run answers finished, with every step recorded in order
    let (status, run) = call(
        &router,
        Method::POST,
        &format!("/automation-rules/{id}/execute"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{run}");
    assert_eq!(run["status"], json!("completed"));
    assert_eq!(run["step_results"].as_array().map(Vec::len), Some(2));
    assert_eq!(run["error"], Value::Null);

    // switching the rule off refuses the next run rather than quietly doing nothing
    let (status, off) = call(
        &router,
        Method::POST,
        &format!("/automation-rules/{id}/toggle"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{off}");
    assert_eq!(off["enabled"], json!(false));
    let (status, refused) = call(
        &router,
        Method::POST,
        &format!("/automation-rules/{id}/execute"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{refused}");
    let (_, _) = call(
        &router,
        Method::POST,
        &format!("/automation-rules/{id}/toggle"),
        json!({}),
    )
    .await;

    // a rule waiting on a step that never ran is recorded as failed, with the reason kept
    let (_, broken) = call(
        &router,
        Method::POST,
        "/automation-rules",
        json!({"name": "Broken", "company_id": "co-1",
               "steps": [{"step_id": "s1", "step_name": "Report", "action": "report",
                          "depends_on": ["gone"]}]}),
    )
    .await;
    let broken_id = broken["id"].as_str().expect("an id").to_string();
    let (status, failed) = call(
        &router,
        Method::POST,
        &format!("/automation-rules/{broken_id}/execute"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{failed}");
    assert_eq!(failed["status"], json!("failed"));
    assert_eq!(
        failed["error"],
        json!("Dependency gone not completed for step s1")
    );

    // the runs are readable, narrowed by status, and one by id
    let (_, runs) = call(
        &router,
        Method::GET,
        "/automation-executions?status=failed",
        json!({}),
    )
    .await;
    assert_eq!(runs.as_array().map(Vec::len), Some(1));
    let run_id = run["id"].as_str().expect("an id").to_string();
    let (status, found) = call(
        &router,
        Method::GET,
        &format!("/automation-executions/{run_id}"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{found}");
    assert_eq!(found["id"], json!(run_id));
    let (status, _) = call(
        &router,
        Method::GET,
        "/automation-executions/nope",
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // deleting answers what it found, and a second delete answers false rather than missing
    let (status, gone) = call(
        &router,
        Method::DELETE,
        &format!("/automation-rules/{id}"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{gone}");
    assert_eq!(gone["deleted"], json!(true));
    let (status, again) = call(
        &router,
        Method::DELETE,
        &format!("/automation-rules/{id}"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{again}");
    assert_eq!(again["deleted"], json!(false));
    let (status, _) = call(
        &router,
        Method::GET,
        &format!("/automation-rules/{id}"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
