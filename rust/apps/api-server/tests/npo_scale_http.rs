//! npo-scale-service over HTTP: the organisation register, credit purchases, and the creditors report.

use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use serde_json::{json, Value};
use store::{Graph, StoreError};
use tower::ServiceExt;

struct Scripted {
    queue: Mutex<Vec<Vec<Value>>>,
    seen: Mutex<Vec<(String, Value)>>,
}

impl Scripted {
    fn queue(scripts: Vec<Vec<Value>>) -> Arc<Self> {
        Arc::new(Self {
            queue: Mutex::new(scripts),
            seen: Mutex::new(Vec::new()),
        })
    }

    fn last(&self) -> (String, Value) {
        self.seen
            .lock()
            .unwrap()
            .last()
            .cloned()
            .unwrap_or_else(|| (String::new(), Value::Null))
    }
}

impl Graph for Scripted {
    fn run(&self, cypher: &str, params: Value) -> Result<Vec<Value>, StoreError> {
        self.seen.lock().unwrap().push((cypher.to_owned(), params));
        let mut queue = self.queue.lock().unwrap();
        if queue.is_empty() {
            return Ok(Vec::new());
        }
        Ok(queue.remove(0))
    }
}

async fn call(graph: Arc<Scripted>, method: Method, uri: &str) -> (StatusCode, Value) {
    let router = vimbai_api::records::router(graph);
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header("x-user-id", "user-1")
        .header("x-book-id", "book-1")
        .body(Body::empty())
        .unwrap();
    let response = router.oneshot(request).await.unwrap();
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
async fn an_organisation_is_created_at_the_band_its_revenue_earns() {
    let graph = Scripted::queue(vec![vec![json!({"id": "org-1"})]]);
    let (status, body) = call(
        graph.clone(),
        Method::POST,
        "/npo-scale/orgs?name=Harare%20Youth%20Trust&org_type=nonprofit&annual_revenue=60000",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let (cypher, params) = graph.last();
    assert!(cypher.contains("NPOOrg"), "{cypher}");
    assert_eq!(params["props"]["size_band"], json!("medium"));
    assert_eq!(params["props"]["owner_id"], json!("user-1"));
}

#[tokio::test]
async fn another_tenants_organisation_is_a_miss() {
    let graph = Scripted::queue(vec![vec![json!({"id": "org-1", "owner_id": "user-2"})]]);
    let (status, _) = call(graph, Method::GET, "/npo-scale/orgs/org-1").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn the_features_carry_the_approval_limit_of_the_band() {
    let graph = Scripted::queue(vec![vec![json!({
        "id": "org-1",
        "owner_id": "user-1",
        "org_type": "nonprofit",
        "size_band": "medium",
    })]]);
    let (status, body) = call(graph, Method::GET, "/npo-scale/orgs/org-1/features").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["approval_limit"], json!(5000));
    assert!(body["features"]
        .as_array()
        .unwrap()
        .iter()
        .any(|f| f == "donor_crm"));
}

#[tokio::test]
async fn creditors_are_what_is_still_owed() {
    let graph = Scripted::queue(vec![
        vec![
            json!({"id": "org-1", "owner_id": "user-1", "org_type": "nonprofit", "size_band": "small"}),
        ],
        vec![json!({"id": "v-1", "org_id": "org-1", "name": "Stationery"})],
        vec![
            json!({"id": "p-1", "org_id": "org-1", "vendor_id": "v-1", "status": "unpaid", "amount": "120.00"}),
            json!({"id": "p-2", "org_id": "org-1", "vendor_id": "v-1", "status": "paid", "amount": "80.00"}),
        ],
    ]);
    let (status, body) = call(
        graph,
        Method::GET,
        "/npo-scale/orgs/org-1/reports/creditors",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["total_owed"], json!("120.000000"));
    assert_eq!(body["creditors"][0]["vendor"], json!("Stationery"));
}

#[tokio::test]
async fn a_purchase_paid_twice_is_refused() {
    let graph = Scripted::queue(vec![
        vec![
            json!({"id": "org-1", "owner_id": "user-1", "org_type": "nonprofit", "size_band": "small"}),
        ],
        vec![json!({"id": "p-1", "org_id": "org-1", "status": "paid", "amount": "120.00"})],
    ]);
    let (status, body) = call(
        graph,
        Method::POST,
        "/npo-scale/orgs/org-1/purchases/p-1/pay",
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(body["error"].as_str().unwrap().contains("Already paid"));
}

#[tokio::test]
async fn a_purchase_with_no_amount_is_refused() {
    let graph = Scripted::queue(vec![vec![json!({
        "id": "org-1",
        "owner_id": "user-1",
        "org_type": "nonprofit",
        "size_band": "small",
    })]]);
    let (status, body) = call(
        graph,
        Method::POST,
        "/npo-scale/orgs/org-1/purchases?vendor_id=v-1&amount=0",
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(body["error"].as_str().unwrap().contains("positive"));
}
