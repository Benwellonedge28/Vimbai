//! workflow-service over HTTP: a definition, a walk through it, and the tasks each step raises.

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use serde_json::{json, Value};
use std::sync::Arc;
use store::fake::MemoryGraph;
use tower::ServiceExt;

struct Book {
    router: axum::Router,
}

impl Book {
    fn new() -> Self {
        Self {
            router: vimbai_api::records::router(Arc::new(MemoryGraph::new())),
        }
    }

    async fn call(&self, method: Method, uri: &str, payload: Value) -> (StatusCode, Value) {
        let request = Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json")
            .header("x-user-id", "user-1")
            .header("x-book-id", "book-1")
            .body(Body::from(payload.to_string()))
            .unwrap();
        let response = self.router.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 1_000_000)
            .await
            .unwrap();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    async fn get(&self, uri: &str) -> (StatusCode, Value) {
        let request = Request::builder()
            .method(Method::GET)
            .uri(uri)
            .header("x-user-id", "user-1")
            .header("x-book-id", "book-1")
            .body(Body::empty())
            .unwrap();
        let response = self.router.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 1_000_000)
            .await
            .unwrap();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }
}

#[tokio::test]
async fn a_definition_is_walked_from_its_first_step_to_its_last() {
    let book = Book::new();
    let (status, made) = book
        .call(
            Method::POST,
            "/workflow-definitions",
            json!({
                "name": "Purchase approval",
                "steps": [
                    {"step_id": "check", "assignee_role": "clerk", "config": {"limit": 500},
                     "next_steps": ["sign"]},
                    {"step_id": "sign", "assignee_role": "director", "next_steps": []}
                ],
                "created_at": "2026-01-01T09:00:00Z"
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{made}");
    let definition_id = made["id"].as_str().expect("an id").to_string();
    assert_eq!(made["is_active"], json!(true));

    let (status, listed) = book.get("/workflow-definitions").await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    assert_eq!(listed.as_array().map(|held| held.len()), Some(1));

    let (status, started) = book
        .call(
            Method::POST,
            &format!("/workflow-definitions/{definition_id}/start"),
            json!({"now": "2026-01-02T09:00:00Z", "context": {"invoice": "INV-1"}}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{started}");
    let instance_id = started["id"].as_str().expect("an id").to_string();
    assert_eq!(started["status"], json!("running"));
    // the first step in the list is the way in
    assert_eq!(started["tasks"][0]["step_id"], json!("check"));
    assert_eq!(started["tasks"][0]["payload"], json!({"limit": 500}));
    let first_task = started["tasks"][0]["task_id"]
        .as_str()
        .expect("a task")
        .to_string();

    let (status, after) = book
        .call(
            Method::POST,
            &format!("/workflow-instances/{instance_id}/complete"),
            json!({"task_id": first_task, "action": "approve", "completed_by_user_id": "user-2",
                   "now": "2026-01-02T10:00:00Z"}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{after}");
    assert_eq!(after["tasks"][1]["step_id"], json!("sign"));
    let second_task = after["tasks"][1]["task_id"]
        .as_str()
        .expect("a task")
        .to_string();

    let (status, done) = book
        .call(
            Method::POST,
            &format!("/workflow-instances/{instance_id}/complete"),
            json!({"task_id": second_task, "action": "approve", "now": "2026-01-02T11:00:00Z"}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{done}");
    assert_eq!(done["status"], json!("completed"));
    assert_eq!(done["end_date"], json!("2026-01-02T11:00:00Z"));

    // a definition switched off cannot be started, and a definition with no steps has no way in
    let (status, switched) = book
        .call(
            Method::PUT,
            &format!("/workflow-definitions/{definition_id}"),
            json!({"is_active": false, "updated_at": "2026-01-03T09:00:00Z"}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{switched}");
    assert_eq!(switched["is_active"], json!(false));
    // the steps survive a change that did not mention them
    assert_eq!(switched["steps"].as_array().map(|held| held.len()), Some(2));

    let (status, _) = book
        .call(
            Method::POST,
            &format!("/workflow-definitions/{definition_id}/start"),
            json!({"now": "2026-01-03T10:00:00Z"}),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    // an instance is readable and removable by its owner
    let (status, held) = book
        .get(&format!("/workflow-instances/{instance_id}"))
        .await;
    assert_eq!(status, StatusCode::OK, "{held}");
    assert_eq!(held["workflow_definition_id"], json!(definition_id));

    let (status, gone) = book
        .call(
            Method::DELETE,
            &format!("/workflow-instances/{instance_id}"),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{gone}");
    assert_eq!(gone["ok"], json!(true));

    let (status, _) = book
        .get(&format!("/workflow-instances/{instance_id}"))
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
