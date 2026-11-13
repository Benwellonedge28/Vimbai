//! family-community-group-service over HTTP: the paths the Python service published, and the four figures the
//! register owns rather than the caller.

use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use serde_json::{json, Value};
use store::{Graph, StoreError};
use tower::ServiceExt;

struct Scripted {
    queue: Mutex<Vec<Vec<Value>>>,
    rows: Vec<Value>,
    seen: Mutex<Vec<(String, Value)>>,
}

impl Scripted {
    fn queue(scripts: Vec<Vec<Value>>) -> Arc<Self> {
        Arc::new(Self {
            queue: Mutex::new(scripts),
            rows: Vec::new(),
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
            return Ok(self.rows.clone());
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
async fn a_group_is_created_with_the_register_owning_its_count_cycle_and_pool() {
    let graph = Scripted::queue(vec![vec![json!({"id": "g-1"})]]);
    let (status, body) = call(
        graph.clone(),
        Method::POST,
        "/family-community-group/groups?name=Chikafu&contribution_amount=25&member_count=9&current_cycle=4",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let (cypher, params) = graph.last();
    assert!(cypher.contains("CommunityGroup"), "{cypher}");
    assert!(cypher.contains("OWNS_GROUP"), "{cypher}");
    assert_eq!(params["props"]["member_count"], json!(0));
    assert_eq!(params["props"]["current_cycle"], json!(1));
    assert_eq!(params["props"]["total_pool"], json!("0.000000"));
    assert_eq!(params["props"]["book_id"], json!("book-1"));
}

#[tokio::test]
async fn a_frequency_the_group_does_not_meet_at_is_refused() {
    let graph = Scripted::queue(vec![]);
    let (status, body) = call(
        graph,
        Method::POST,
        "/family-community-group/groups?name=Chikafu&contribution_frequency=daily",
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(body["error"]
        .as_str()
        .unwrap()
        .contains("Invalid frequency"));
}

#[tokio::test]
async fn a_contribution_the_group_holds_the_member_for_is_recorded_and_raises_the_pool() {
    let graph = Scripted::queue(vec![
        vec![json!({"id": "g-1", "current_cycle": 2, "total_pool": "100.000000"})],
        vec![json!({"id": "m-1", "group_id": "g-1"})],
        vec![json!({"id": "c-1", "amount": "12.500000", "cycle_number": 2})],
        vec![json!({"id": "g-1", "total_pool": "112.500000"})],
    ]);
    let (status, body) = call(
        graph.clone(),
        Method::POST,
        "/family-community-group/groups/g-1/contribute?member_id=m-1&amount=12.5",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["amount"], json!("12.500000"));

    let (cypher, params) = graph.last();
    assert!(cypher.contains("SET x += $patch"), "{cypher}");
    assert_eq!(params["patch"]["total_pool"], json!("112.500000"));
}

#[tokio::test]
async fn a_cycle_advances_by_one_and_answers_where_it_landed() {
    let graph = Scripted::queue(vec![
        vec![json!({"id": "g-1", "current_cycle": 3})],
        vec![json!({"id": "g-1", "current_cycle": 4})],
    ]);
    let (status, body) = call(
        graph,
        Method::POST,
        "/family-community-group/groups/g-1/advance-cycle",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body, json!({"group_id": "g-1", "current_cycle": 4}));
}

#[tokio::test]
async fn a_group_the_caller_does_not_hold_is_a_miss() {
    let graph = Scripted::queue(vec![vec![]]);
    let (status, _) = call(graph, Method::GET, "/family-community-group/groups/g-1").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn the_payout_list_answers_the_groups_schedule() {
    let graph = Scripted::queue(vec![vec![
        json!({"id": "p-1", "group_id": "g-1", "amount": "240.000000"}),
        json!({"id": "p-2", "group_id": "g-2", "amount": "90.000000"}),
    ]]);
    let (status, body) = call(
        graph,
        Method::GET,
        "/family-community-group/groups/g-1/payouts",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["count"], json!(1));
    assert_eq!(body["payouts"][0]["id"], json!("p-1"));
}
