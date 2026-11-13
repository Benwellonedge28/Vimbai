//! tax-accounting-service over HTTP: rates, the tax on a transaction, returns, withholding and the reports.

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

    /// The statement that was run last, with the parameters it was run with.
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

async fn call(
    graph: Arc<Scripted>,
    method: Method,
    uri: &str,
    payload: Option<Value>,
) -> (StatusCode, Value) {
    let router = vimbai_api::records::router(graph);
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .header("x-user-id", "user-1")
        .header("x-book-id", "book-1");
    let body = match payload {
        Some(value) => Body::from(value.to_string()),
        None => Body::empty(),
    };
    let request = request.body(body).unwrap();
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

async fn send(graph: Arc<Scripted>, uri: &str, body: Value) -> (StatusCode, Value) {
    call(graph, Method::POST, uri, Some(body)).await
}

#[tokio::test]
async fn a_tax_rate_is_stored_and_echoed() {
    let graph = Scripted::queue(vec![vec![json!({"id": "rate-1"})]]);
    let (status, body) = send(
        graph.clone(),
        "/tax-rates",
        json!({
            "tax_type": "vat",
            "jurisdiction": "ZW",
            "jurisdiction_type": "federal",
            "rate_type": "standard",
            "rate_percentage": 15,
            "effective_from": "2026-01-01T00:00:00",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["id"], json!("rate-1"));
    assert_eq!(body["rate_percentage"], json!(15));
    assert_eq!(body["is_active"], json!(true));
}

#[tokio::test]
async fn a_rate_without_a_tax_type_is_refused() {
    let graph = Scripted::queue(vec![Vec::new()]);
    let (status, _) = send(
        graph,
        "/tax-rates",
        json!({"jurisdiction": "ZW", "rate_percentage": 15}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn the_rates_are_listed_with_a_count() {
    let graph = Scripted::queue(vec![vec![
        json!({"id": "rate-1", "tax_type": "vat", "jurisdiction": "ZW", "is_active": true}),
        json!({"id": "rate-2", "tax_type": "gst", "jurisdiction": "ZW", "is_active": true}),
    ]]);
    let (status, body) = call(graph, Method::GET, "/tax-rates?tax_type=vat", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["count"], json!(1));
    assert_eq!(body["rates"][0]["id"], json!("rate-1"));
}

#[tokio::test]
async fn a_calculation_answers_the_three_amounts_and_stores_the_transaction() {
    let graph = Scripted::queue(vec![
        // no registered rate, so the default table answers
        Vec::new(),
        vec![json!({"id": "txn-1"})],
    ]);
    let (status, body) = send(
        graph.clone(),
        "/calculate",
        json!({
            "transaction_type": "sale",
            "tax_type": "vat",
            "jurisdiction": "ZW",
            "gross_amount": 120,
            "transaction_date": "2026-03-01",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["net_amount"], json!("100.000000"));
    assert_eq!(body["tax_amount"], json!("20.000000"));

    let (statement, params) = graph.last();
    assert!(
        statement.contains("CREATE (x:TaxTransaction)"),
        "the transaction was not stored: {statement}"
    );
    assert_eq!(params["props"]["book_id"], json!("book-1"));
    assert_eq!(params["props"]["user_id"], json!("user-1"));
}

#[tokio::test]
async fn a_calculation_sent_as_query_parameters_is_understood() {
    let graph = Scripted::queue(vec![Vec::new(), vec![json!({"id": "txn-1"})]]);
    let (status, body) = call(
        graph,
        Method::POST,
        "/calculate?transaction_type=sale&tax_type=vat&jurisdiction=ZW&gross_amount=120&transaction_date=2026-03-01",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["tax_amount"], json!("20.000000"));
}

#[tokio::test]
async fn a_batch_answers_once_per_transaction() {
    // each calculation reads the rates and then stores its transaction
    let graph = Scripted::queue(vec![
        Vec::new(),
        vec![json!({"id": "txn-1"})],
        Vec::new(),
        vec![json!({"id": "txn-2"})],
    ]);
    let (status, body) = send(
        graph,
        "/calculate-batch",
        json!([
            {"transaction_type": "sale", "tax_type": "vat", "jurisdiction": "ZW",
             "gross_amount": 120, "transaction_date": "2026-03-01"},
            {"transaction_type": "purchase", "tax_type": "vat", "jurisdiction": "ZW",
             "gross_amount": 60, "transaction_date": "2026-03-02"},
        ]),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body.as_array().map(|rows| rows.len()), Some(2));
}

#[tokio::test]
async fn a_return_is_measured_from_its_transactions_over_http() {
    let graph = Scripted::queue(vec![
        vec![
            json!({"id": "t1", "tax_type": "vat", "jurisdiction": "ZW", "transaction_type": "sale",
                   "transaction_date": "2026-03-05", "gross_amount": "120", "tax_amount": "20"}),
            json!({"id": "t2", "tax_type": "vat", "jurisdiction": "ZW", "transaction_type": "purchase",
                   "transaction_date": "2026-03-06", "gross_amount": "60", "tax_amount": "10"}),
        ],
        vec![json!({"id": "ret-1"})],
    ]);
    let (status, body) = send(
        graph,
        "/tax-returns",
        json!({
            "tax_type": "vat",
            "jurisdiction": "ZW",
            "period_start": "2026-03-01T00:00:00",
            "period_end": "2026-03-31T00:00:00",
            "filing_frequency": "monthly",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["tax_collected"], json!("20.000000"));
    assert_eq!(body["tax_paid"], json!("10.000000"));
    assert_eq!(body["net_tax_due"], json!("10.000000"));
}

#[tokio::test]
async fn filing_a_return_marks_it_filed_on_the_day_the_caller_gave() {
    let graph = Scripted::queue(vec![vec![json!({"id": "ret-1", "status": "filed"})]]);
    let (status, body) = send(
        graph.clone(),
        "/tax-returns/ret-1/file",
        json!({"today": "2026-04-15"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (statement, params) = graph.last();
    assert!(
        statement.contains("x:TaxReturn"),
        "the return was not updated: {statement}"
    );
    assert_eq!(params["patch"]["status"], json!("filed"));
    assert_eq!(params["patch"]["filed_date"], json!("2026-04-15"));
}

#[tokio::test]
async fn a_withheld_amount_is_worked_out_when_the_caller_states_only_a_rate() {
    let graph = Scripted::queue(vec![vec![json!({"id": "wht-1"})]]);
    let (status, body) = send(
        graph,
        "/withholding-tax",
        json!({
            "recipient_id": "r-1",
            "recipient_name": "Kaguvi Consulting",
            "payment_date": "2026-03-01",
            "gross_amount": 200,
            "withholding_rate": 15,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["withholding_amount"], json!("30.000000"));
    assert_eq!(body["net_amount_paid"], json!("170.000000"));
}

#[tokio::test]
async fn the_tax_summary_counts_sales_against_purchases() {
    let graph = Scripted::queue(vec![
        vec![
            json!({"id": "t1", "tax_type": "vat", "jurisdiction": "ZW", "transaction_type": "sale",
                   "transaction_date": "2026-03-05", "gross_amount": "120", "tax_amount": "20"}),
            json!({"id": "t2", "tax_type": "vat", "jurisdiction": "ZW", "transaction_type": "purchase",
                   "transaction_date": "2026-03-06", "gross_amount": "60", "tax_amount": "10"}),
        ],
        vec![json!({"id": "report-1"})],
    ]);
    let (status, body) = send(
        graph,
        "/reports/tax-summary",
        json!({
            "tax_type": "vat",
            "jurisdiction": "ZW",
            "period_start": "2026-03-01T00:00:00",
            "period_end": "2026-03-31T00:00:00",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["summary"]["total_transactions"], json!(2));
    assert_eq!(body["summary"]["net_tax_position"], json!("10.000000"));
    assert_eq!(body["total_tax"], json!("30.000000"));
}

#[tokio::test]
async fn the_vat_breakdown_groups_by_jurisdiction() {
    let graph = Scripted::queue(vec![vec![
        json!({"tax_type": "vat", "jurisdiction": "ZW", "transaction_type": "sale",
               "transaction_date": "2026-03-05", "tax_amount": "20"}),
        json!({"tax_type": "gst", "jurisdiction": "ZA", "transaction_type": "sale",
               "transaction_date": "2026-03-07", "tax_amount": "15"}),
        // an income tax transaction is not part of a VAT breakdown
        json!({"tax_type": "income_tax", "jurisdiction": "ZW", "transaction_type": "sale",
               "transaction_date": "2026-03-08", "tax_amount": "500"}),
    ]]);
    let (status, body) = call(
        graph,
        Method::GET,
        "/reports/vat-by-jurisdiction?period_start=2026-03-01T00:00:00&period_end=2026-03-31T00:00:00",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["by_jurisdiction"]["ZW"]["collected"],
        json!("20.000000")
    );
    assert_eq!(
        body["by_jurisdiction"]["ZA"]["collected"],
        json!("15.000000")
    );
}

#[tokio::test]
async fn deferred_tax_is_split_between_assets_and_liabilities() {
    let graph = Scripted::queue(vec![vec![json!({"id": "dt-1"})]]);
    let (status, body) = send(
        graph,
        "/deferred-tax",
        json!({
            "period_end": "2026-03-31T00:00:00",
            "tax_rate": 25,
            "temporary_differences": [
                {"type": "asset", "amount": 400},
                {"type": "liability", "amount": 200},
            ],
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["deferred_tax_asset"], json!("100.000000"));
    assert_eq!(body["deferred_tax_liability"], json!("50.000000"));
}

#[tokio::test]
async fn the_liability_schedule_counts_only_filed_returns_as_outstanding() {
    let graph = Scripted::queue(vec![vec![
        json!({"id": "r1", "tax_type": "vat", "jurisdiction": "ZW", "status": "filed", "net_tax_due": "10"}),
        json!({"id": "r2", "tax_type": "vat", "jurisdiction": "ZW", "status": "draft", "net_tax_due": "99"}),
    ]]);
    let (status, body) = call(graph, Method::GET, "/liability-schedule", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["total_outstanding"], json!("10.000000"));
    assert_eq!(body["count"], json!(2));
}
