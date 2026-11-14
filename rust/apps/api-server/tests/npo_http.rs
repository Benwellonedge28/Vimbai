#![forbid(unsafe_code)]
//! npo-service over HTTP: the paths the Python service published, and the figures a non-profit's register owns.

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
async fn a_fund_is_created_on_its_opening_balance() {
    let graph = Scripted::queue(vec![vec![], vec![json!({"id": "f-1"})]]);
    let (status, body) = call(
        graph.clone(),
        Method::POST,
        "/npo/funds/?fund_code=GEN-1&fund_name=General&fund_type=general&purpose=running&initial_balance=500",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let (cypher, params) = graph.last();
    assert!(cypher.contains("NPOFund"), "{cypher}");
    assert!(cypher.contains("OWNS_NPO_FUND"), "{cypher}");
    assert_eq!(params["props"]["current_balance"], json!("500.000000"));
    assert_eq!(params["props"]["total_contributions"], json!("0.000000"));
    assert_eq!(params["props"]["book_id"], json!("book-1"));
}

#[tokio::test]
async fn a_second_fund_with_the_same_code_is_refused() {
    let graph = Scripted::queue(vec![vec![json!({"id": "f-1", "fund_code": "GEN-1"})]]);
    let (status, body) = call(
        graph,
        Method::POST,
        "/npo/funds/?fund_code=GEN-1&fund_name=Another&fund_type=general",
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(body["error"].as_str().unwrap().contains("already exists"));
}

#[tokio::test]
async fn a_disbursement_lowers_the_balance_and_raises_what_was_disbursed() {
    let graph = Scripted::queue(vec![
        vec![
            json!({"id": "f-1", "fund_type": "general", "current_balance": "1000.00", "total_contributions": "1000.00", "total_disbursements": "0.00"}),
        ],
        vec![json!({"id": "tx-1"})],
        vec![json!({"id": "f-1"})],
    ]);
    let (status, body) = call(
        graph.clone(),
        Method::POST,
        "/npo/funds/f-1/transactions/?transaction_type=disbursement&amount=250&transaction_date=2026-03-02",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["balance_after"], json!("750.000000"));

    let (_, params) = graph.last();
    assert_eq!(params["patch"]["current_balance"], json!("750.000000"));
    assert_eq!(params["patch"]["total_disbursements"], json!("250.000000"));
}

#[tokio::test]
async fn an_endowment_refuses_a_disbursement() {
    let graph = Scripted::queue(vec![vec![json!({
        "id": "f-1",
        "fund_type": "endowment",
        "current_balance": "5000.00",
        "total_contributions": "5000.00",
        "total_disbursements": "0.00",
    })]]);
    let (status, body) = call(
        graph,
        Method::POST,
        "/npo/funds/f-1/transactions/?transaction_type=disbursement&amount=10",
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(body["error"].as_str().unwrap().contains("endowment fund"));
}

#[tokio::test]
async fn net_assets_are_read_from_the_active_funds() {
    let graph = Scripted::queue(vec![vec![
        json!({"id": "f-1", "fund_type": "general", "status": "active", "current_balance": "400.00"}),
        json!({"id": "f-2", "fund_type": "temporarily_restricted", "status": "active", "current_balance": "100.00"}),
    ]]);
    let (status, body) = call(graph, Method::GET, "/npo/net-assets/2026-03-31").await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
}

#[tokio::test]
async fn the_statement_of_activities_reports_the_change_in_net_assets() {
    let graph = Scripted::queue(vec![
        vec![json!({"id": "d-1", "donation_date": "2026-02-01", "amount": "800.00"})],
        vec![json!({"id": "g-1", "status": "active", "amount_received": "1200.00"})],
        vec![
            json!({"id": "t-1", "transaction_type": "disbursement", "transaction_date": "2026-02-10", "amount": "500.00"}),
        ],
        vec![json!({"id": "s-1"})],
    ]);
    let (status, body) = call(
        graph.clone(),
        Method::POST,
        "/npo/statements/activities/?period_start=2026-01-01&period_end=2026-03-31",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["total_revenue"], json!("2000.000000"));
    assert_eq!(body["change_in_net_assets"], json!("1500.000000"));

    let (cypher, _) = graph.last();
    assert!(cypher.contains("StatementOfActivities"), "{cypher}");
}

#[tokio::test]
async fn an_sroi_analysis_answers_a_ratio_without_keeping_it() {
    let graph = Scripted::queue(vec![]);
    let (status, body) = call(
        graph.clone(),
        Method::POST,
        "/npo/sroi-analyses/?total_investment=1000&social_value_created=3400",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["analysis"]["social_return_on_investment"],
        json!("3.4000")
    );
    // nothing was written
    assert_eq!(graph.last().1, Value::Null);
}

#[tokio::test]
async fn a_fund_the_caller_does_not_hold_is_a_miss() {
    let graph = Scripted::queue(vec![vec![]]);
    let (status, _) = call(graph, Method::GET, "/npo/funds/f-1").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// --- the paths that used to belong to npo-service alone: donations and grants -------------------

#[tokio::test]
async fn a_donation_is_recorded_with_its_amount_as_money() {
    let graph = Scripted::queue(vec![vec![json!({"id": "d-1"})]]);
    let (status, body) = call(
        graph.clone(),
        Method::POST,
        "/npo/donations/?donor_name=Wellwisher&amount=250&donation_date=2026-03-01&fund_id=f-1",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["id"], json!("d-1"));

    let (cypher, params) = graph.last();
    assert!(cypher.contains("Donation"), "{cypher}");
    assert!(cypher.contains("OWNS_DONATION"), "{cypher}");
    assert_eq!(params["props"]["amount"], json!("250.000000"));
    assert_eq!(params["props"]["book_id"], json!("book-1"));
}

#[tokio::test]
async fn a_donation_of_nothing_is_refused() {
    let graph = Scripted::queue(vec![vec![]]);
    let (status, body) = call(
        graph,
        Method::POST,
        "/npo/donations/?donor_name=Wellwisher&amount=0",
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(body["error"]
        .as_str()
        .unwrap()
        .contains("must be an amount"));
}

#[tokio::test]
async fn donations_are_listed_under_a_count() {
    let graph = Scripted::queue(vec![vec![
        json!({"id": "d-1", "amount": "250.000000"}),
        json!({"id": "d-2", "amount": "50.000000"}),
    ]]);
    let (status, body) = call(graph, Method::GET, "/npo/donations/").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["count"], json!(2));
    assert_eq!(body["donations"][1]["id"], json!("d-2"));
}

#[tokio::test]
async fn a_grant_starts_active_with_nothing_received() {
    let graph = Scripted::queue(vec![vec![json!({"id": "g-1"})]]);
    let (status, body) = call(
        graph.clone(),
        Method::POST,
        "/npo/grants/?grant_name=Youth&grantor_name=Trust&amount=10000&start_date=2026-01-01&end_date=2026-12-31",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let (cypher, params) = graph.last();
    assert!(cypher.contains("Grant"), "{cypher}");
    assert!(cypher.contains("OWNS_GRANT"), "{cypher}");
    assert_eq!(params["props"]["amount"], json!("10000.000000"));
    assert_eq!(params["props"]["amount_received"], json!("0.000000"));
    assert_eq!(params["props"]["status"], json!("active"));
}

#[tokio::test]
async fn grants_can_be_narrowed_to_one_status() {
    let graph = Scripted::queue(vec![vec![
        json!({"id": "g-1", "status": "active"}),
        json!({"id": "g-2", "status": "closed"}),
    ]]);
    let (status, body) = call(graph, Method::GET, "/npo/grants/?status=active").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["count"], json!(1));
    assert_eq!(body["grants"][0]["id"], json!("g-1"));
}

#[tokio::test]
async fn one_grant_is_read_by_id() {
    let graph = Scripted::queue(vec![vec![json!({"id": "g-1", "grant_name": "Youth"})]]);
    let (status, body) = call(graph, Method::GET, "/npo/grants/g-1").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["grant_name"], json!("Youth"));
}
