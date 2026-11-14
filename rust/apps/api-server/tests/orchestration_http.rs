#![forbid(unsafe_code)]
//! distributed-workflow-orchestration-service over HTTP: a workflow, its run, and its rollback.

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
async fn a_workflow_runs_its_steps_and_rolls_back_only_what_it_can_undo() {
    let router = router();

    let (status, made) = call(
        &router,
        Method::POST,
        "/workflows",
        json!({"name": "Month end across services",
        "steps": [
            {"name": "Post accruals", "service_url": "http://accounting", "endpoint": "/journals",
             "compensation_url": "http://accounting/journals/reverse"},
            {"name": "Reconcile bank", "service_url": "http://banking", "endpoint": "/reconcile"},
            {"name": "Produce report", "service_url": "http://reporting", "endpoint": "/reports"}
        ]}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{made}");
    assert_eq!(made["status"], json!("pending"));
    assert_eq!(made["steps"][0]["method"], json!("POST"));
    assert_eq!(made["steps"][0]["max_retries"], json!(3));
    let id = made["id"].as_str().expect("an id").to_string();

    // the listing narrows by state
    let (status, pending) =
        call(&router, Method::GET, "/workflows?status=pending", json!({})).await;
    assert_eq!(status, StatusCode::OK, "{pending}");
    assert_eq!(pending.as_array().map(Vec::len), Some(1));

    // a run records the steps and says plainly that nothing was sent
    let (status, run) = call(
        &router,
        Method::POST,
        &format!("/workflows/{id}/execute"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{run}");
    assert_eq!(run["status"], json!("completed"));
    assert_eq!(run["steps_completed"], json!(3));
    assert_eq!(run["requests_sent"], json!(0));

    // a completed workflow cannot be run a second time
    let (status, _) = call(
        &router,
        Method::POST,
        &format!("/workflows/{id}/execute"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // the rollback undoes only the step that completed and carries a compensation address
    let (status, rolled) = call(
        &router,
        Method::POST,
        &format!("/workflows/{id}/compensate"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{rolled}");
    assert_eq!(rolled["steps_compensated"], json!(1));

    let (status, after) = call(&router, Method::GET, &format!("/workflows/{id}"), json!({})).await;
    assert_eq!(status, StatusCode::OK, "{after}");
    assert_eq!(after["status"], json!("compensated"));
    assert_eq!(after["steps"][0]["status"], json!("compensated"));
    assert_eq!(after["steps"][1]["status"], json!("completed"));

    // cancelling closes the workflow out, and it cannot then be run
    let (status, closed) = call(
        &router,
        Method::DELETE,
        &format!("/workflows/{id}"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{closed}");
    assert_eq!(closed["status"], json!("cancelled"));
    let (status, _) = call(
        &router,
        Method::POST,
        &format!("/workflows/{id}/execute"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // a workflow that is not there is missing, on every door
    for uri in [
        "/workflows/nope",
        "/workflows/nope/execute",
        "/workflows/nope/compensate",
    ] {
        let method = if uri == "/workflows/nope" {
            Method::GET
        } else {
            Method::POST
        };
        let (status, _) = call(&router, method, uri, json!({})).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri}");
    }
}
